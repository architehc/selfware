//! Token Budget Management for Code Introspection
//!
//! Tracks token usage and makes intelligent decisions about depth levels
//! to maximize code coverage within budget constraints.

use anyhow::Result;
use std::path::Path;

use crate::token_count::estimate_content_tokens;

/// Manages token allocation for code introspection operations
#[derive(Debug, Clone)]
pub struct TokenBudget {
    total: usize,
    used: usize,
    reserved: usize,
}

/// Estimated token cost for a file at different depths
#[derive(Debug, Clone)]
pub struct FileEstimate {
    pub min_tokens: usize,
    pub tokens: usize,
    pub max_tokens: usize,
}

/// Depth levels for code introspection
#[derive(Debug, Clone, PartialEq)]
pub enum Depth {
    /// File-level metadata only (~50 tokens/file)
    Overview,
    /// Public API signatures (~200 tokens/file)
    Signatures,
    /// Complete source code (~1000+ tokens/file)
    Full,
    /// Cross-file dependencies (~300 tokens/file)
    Dependencies,
}

impl Depth {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "overview" => Ok(Self::Overview),
            "signatures" => Ok(Self::Signatures),
            "full" => Ok(Self::Full),
            "dependencies" => Ok(Self::Dependencies),
            _ => anyhow::bail!("Unknown depth level: {}", s),
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Self::Overview => "overview".to_string(),
            Self::Signatures => "signatures".to_string(),
            Self::Full => "full".to_string(),
            Self::Dependencies => "dependencies".to_string(),
        }
    }

    /// Default token cost per file at this depth
    pub fn default_tokens(&self) -> usize {
        match self {
            Self::Overview => 50,
            Self::Signatures => 200,
            Self::Full => 1000,
            Self::Dependencies => 300,
        }
    }

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

    /// Allocate tokens from budget, returning actual amount granted
    pub fn allocate(&mut self, requested: usize) -> usize {
        let available = self.remaining();
        let granted = requested.min(available);
        self.used += granted;
        granted
    }

    /// Estimate tokens for a file at given depth
    pub async fn estimate_file(&self, path: &Path, depth: &Depth) -> Result<FileEstimate> {
        // Read file content for accurate estimation
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(_) => {
                // Fallback to default estimates if can't read
                let base = depth.default_tokens();
                return Ok(FileEstimate {
                    min_tokens: base / 2,
                    tokens: base,
                    max_tokens: base * 2,
                });
            }
        };

        let total_tokens = estimate_content_tokens(&content);

        let estimate = match depth {
            Depth::Overview => {
                // Overview is roughly 5% of file content (imports, module structure)
                let tokens = (total_tokens / 20).max(20).min(100);
                FileEstimate {
                    min_tokens: 10,
                    tokens,
                    max_tokens: tokens + 50,
                }
            }
            Depth::Signatures => {
                // Signatures are roughly 20% of file (function signatures without bodies)
                let tokens = (total_tokens / 5).max(50).min(500);
                FileEstimate {
                    min_tokens: 20,
                    tokens,
                    max_tokens: tokens + 200,
                }
            }
            Depth::Full => {
                // Full content
                FileEstimate {
                    min_tokens: total_tokens / 2,
                    tokens: total_tokens,
                    max_tokens: total_tokens + 100,
                }
            }
            Depth::Dependencies => {
                // Dependencies require parsing imports and calls
                let tokens = (total_tokens / 10).max(50).min(500);
                FileEstimate {
                    min_tokens: 30,
                    tokens,
                    max_tokens: tokens + 100,
                }
            }
        };

        Ok(estimate)
    }

    /// Suggest optimal depth level based on file count and budget
    pub fn suggest_depth(&self, files: &[FileMeta]) -> Depth {
        let available = self.remaining();

        if files.is_empty() {
            // Default to signatures when we don't know file count
            return Depth::Signatures;
        }

        // Calculate max coverage at each depth level
        let overview_coverage = available / Depth::Overview.default_tokens();
        let signatures_coverage = available / Depth::Signatures.default_tokens();
        let full_coverage = available / Depth::Full.default_tokens();

        let total_files = files.len();

        // Prefer signatures if it gives good coverage
        if signatures_coverage >= total_files {
            Depth::Signatures
        } else if overview_coverage >= total_files {
            Depth::Overview
        } else if full_coverage >= 1 {
            // Can at least show full content of some files
            Depth::Signatures
        } else {
            Depth::Overview
        }
    }

    /// Suggest depth that fits remaining budget for a specific file
    pub async fn suggest_depth_for_file(&self, path: &Path) -> Result<Depth> {
        let remaining = self.remaining();

        // Try each depth level from most detailed to least
        for depth in [Depth::Full, Depth::Signatures, Depth::Overview] {
            let estimate = self.estimate_file(path, &depth).await?;
            if estimate.tokens <= remaining {
                return Ok(depth);
            }
        }

        // Fallback to overview
        Ok(Depth::Overview)
    }

    /// Check if we can afford a specific operation
    pub fn can_afford(&self, tokens: usize) -> bool {
        tokens <= self.remaining()
    }

    /// Get budget utilization percentage
    pub fn utilization_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used as f64 / self.total as f64) * 100.0
    }

    /// Force allocation even if over budget (for critical operations)
    pub fn force_allocate(&mut self, tokens: usize) {
        self.used += tokens;
    }
}

/// Metadata about a file for depth suggestion
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub size_bytes: u64,
    pub language: Option<String>,
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

    pub fn iteration_exhausted(&self) -> bool {
        self.current_iteration >= self.max_iterations
    }

    pub fn next_iteration(&mut self) {
        self.current_iteration += 1;
    }

    /// Check if we can afford another phase
    pub fn can_continue(&self) -> bool {
        !self.iteration_exhausted() && !self.token_budget.exhausted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_allocation() {
        let mut budget = TokenBudget::new(1000);
        budget.reserve(20); // Reserve 200 tokens

        assert_eq!(budget.remaining(), 800);

        let granted = budget.allocate(500);
        assert_eq!(granted, 500);
        assert_eq!(budget.used(), 500);
        assert_eq!(budget.remaining(), 300);
    }

    #[test]
    fn test_budget_exhaustion() {
        let mut budget = TokenBudget::new(1000);
        budget.reserve(20);

        budget.allocate(800); // Use all available
        assert!(budget.exhausted());

        // Further allocations return 0
        let granted = budget.allocate(100);
        assert_eq!(granted, 0);
    }

    #[test]
    fn test_depth_downgrade() {
        assert!(Depth::Full.downgrade().is_some());
        assert_eq!(Depth::Full.downgrade().unwrap(), Depth::Signatures);
        assert_eq!(Depth::Signatures.downgrade().unwrap(), Depth::Overview);
        assert!(Depth::Overview.downgrade().is_none());
    }

    #[test]
    fn test_plan_budget_iterations() {
        let mut budget = PlanBudget::new(10, 10000);
        assert_eq!(budget.iterations_remaining(), 10);

        budget.next_iteration();
        assert_eq!(budget.current_iteration, 1);
        assert_eq!(budget.iterations_remaining(), 9);
    }
}
