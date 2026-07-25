//! Guardrail Enforcement for SWL Workflows
//!
//! Provides runtime enforcement of guardrails defined in SWL documents.
//! Supports multiple trigger points (pre/post agent/tool/workflow) and
//! various violation actions (block, warn, log, alert).
//!
//! # Example
//!
//! ```rust
//! use selfware::swl::guardrails::{GuardrailEnforcer, GuardrailContext, GuardrailType};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let enforcer = GuardrailEnforcer::new();
//!
//! let ctx = GuardrailContext::new()
//!     .with_agent_output("agent1", "Safe output");
//!
//! let summary = enforcer.check(GuardrailType::PostAgent, &ctx).await?;
//!
//! if summary.should_block() {
//!     println!("Execution blocked by guardrails");
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(test)]
mod integration;

pub mod enforcer;
pub mod engine;
pub mod types;

// Re-export main types for convenience
pub use enforcer::{GuardrailEnforcer, GuardrailStats};
pub use engine::GuardrailEngine;
pub use types::{
    Condition, EvaluationResult, GuardrailContext, GuardrailDef, GuardrailOutcome,
    GuardrailSeverity, GuardrailSummary, GuardrailTelemetryEvent, GuardrailType, LogicalOperator,
    ViolationAction,
};

use crate::errors::SelfwareError;
use crate::swl::parser::ast::SwlDocument;

/// Quick check if a guardrail would pass
pub async fn quick_check(
    condition: &str,
    context: &GuardrailContext,
) -> Result<EvaluationResult, SelfwareError> {
    let engine = GuardrailEngine::new();
    Ok(engine.evaluate_condition(&Condition::Inline(condition.to_string()), context))
}

/// Builder for creating composite guardrail conditions
pub struct ConditionBuilder {
    operator: LogicalOperator,
    conditions: Vec<Condition>,
}

impl ConditionBuilder {
    /// Create a new AND condition builder
    pub fn and() -> Self {
        Self {
            operator: LogicalOperator::And,
            conditions: Vec::new(),
        }
    }

    /// Create a new OR condition builder
    pub fn or() -> Self {
        Self {
            operator: LogicalOperator::Or,
            conditions: Vec::new(),
        }
    }

    /// Add an inline condition
    pub fn inline(mut self, expr: impl Into<String>) -> Self {
        self.conditions.push(Condition::Inline(expr.into()));
        self
    }

    /// Add a code condition
    pub fn code(mut self, language: impl Into<String>, content: impl Into<String>) -> Self {
        self.conditions.push(Condition::Code {
            language: language.into(),
            content: content.into(),
        });
        self
    }

    /// Add a nested composite condition
    pub fn composite(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Build the final condition
    pub fn build(self) -> Condition {
        match self.conditions.len() {
            0 => Condition::Inline("true".to_string()),
            1 => self
                .conditions
                .into_iter()
                .next()
                .unwrap_or_else(|| Condition::Inline("true".to_string())),
            _ => Condition::Composite {
                operator: self.operator,
                conditions: self.conditions,
            },
        }
    }
}

/// Helper functions for common guardrail patterns
pub mod patterns {
    use super::*;

    /// Create a condition that checks for secrets in output
    pub fn no_secrets_in_output() -> Condition {
        ConditionBuilder::and()
            .inline("!agent_output.to_lowercase().contains('password:')")
            .inline("!agent_output.to_lowercase().contains('api_key:')")
            .inline("!agent_output.to_lowercase().contains('secret:')")
            .inline("!agent_output.to_lowercase().contains('token:')")
            .build()
    }

    /// Create a condition that limits output length
    pub fn max_output_length(max_chars: usize) -> Condition {
        Condition::Inline(format!("agent_output.len() <= {}", max_chars))
    }

    /// Create a condition for safe shell commands
    pub fn safe_shell_command() -> Condition {
        ConditionBuilder::and()
            .inline("!tool_input.contains('rm -rf')")
            .inline("!tool_input.contains('mkfs')")
            .inline("!tool_input.contains('dd if=')")
            .inline("!tool_input.contains('> /dev/')")
            .build()
    }

    /// Create a condition for allowed file paths
    pub fn allowed_paths(paths: &[impl AsRef<str>]) -> Condition {
        let path_checks: Vec<String> = paths
            .iter()
            .map(|p| format!("tool_input.starts_with('{}')", p.as_ref()))
            .collect();

        Condition::Inline(format!("({})", path_checks.join(" || ")))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/guardrails/mod_test.rs"]
mod tests;
