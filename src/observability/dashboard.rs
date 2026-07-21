//! Observability dashboard
//!
//! Token usage accounting shared by the agent loop, task runner, and
//! headless CLI (`observability::dashboard::TokenUsage`).

use serde::{Deserialize, Serialize};

/// Token usage tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens
    pub input: usize,
    /// Output tokens
    pub output: usize,
    /// Total tokens
    pub total: usize,
    /// Estimated cost
    pub cost: Option<f64>,
}

impl TokenUsage {
    /// Create new usage
    pub fn new(input: usize, output: usize) -> Self {
        Self {
            input,
            output,
            total: input + output,
            cost: None,
        }
    }

    /// With cost calculation
    pub fn with_cost(mut self, input_rate: f64, output_rate: f64) -> Self {
        self.cost = Some(
            (self.input as f64 / 1000.0) * input_rate + (self.output as f64 / 1000.0) * output_rate,
        );
        self
    }

    /// Add more usage
    pub fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.total += other.total;
        if let (Some(a), Some(b)) = (self.cost, other.cost) {
            self.cost = Some(a + b);
        }
    }

    /// Format for display
    pub fn display(&self) -> String {
        let cost = self
            .cost
            .map(|c| format!(" (${:.4})", c))
            .unwrap_or_default();
        format!(
            "{} tokens ({} in, {} out){}",
            self.total, self.input, self.output, cost
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/observability/dashboard/dashboard_test.rs"]
mod tests;
