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
mod tests {
    use super::*;

    #[test]
    fn test_token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.input, 100);
        assert_eq!(usage.output, 50);
        assert_eq!(usage.total, 150);
    }

    #[test]
    fn test_token_usage_with_cost() {
        let usage = TokenUsage::new(1000, 1000).with_cost(0.01, 0.03);
        assert!(usage.cost.is_some());
        assert_eq!(usage.cost.unwrap(), 0.04);
    }

    #[test]
    fn test_token_usage_add() {
        let mut usage1 = TokenUsage::new(100, 50);
        let usage2 = TokenUsage::new(200, 100);
        usage1.add(&usage2);

        assert_eq!(usage1.input, 300);
        assert_eq!(usage1.output, 150);
        assert_eq!(usage1.total, 450);
    }

    #[test]
    fn test_token_usage_display() {
        let usage = TokenUsage::new(100, 50);
        let display = usage.display();
        assert!(display.contains("150"));
        assert!(display.contains("100 in"));
        assert!(display.contains("50 out"));
    }
}
