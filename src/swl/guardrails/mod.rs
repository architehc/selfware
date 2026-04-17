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

/// Initialize guardrail enforcer from SWL document
pub fn enforcer_from_document(doc: &SwlDocument) -> GuardrailEnforcer {
    let mut enforcer = GuardrailEnforcer::new();
    enforcer.register_guardrails(&doc.guardrails);
    enforcer
}

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

    /// Create a condition that blocks on critical markers
    pub fn no_critical_issues() -> Condition {
        Condition::Inline("!agent_output.contains('[CRITICAL]')".to_string())
    }

    /// Create a condition that requires specific marker
    pub fn requires_marker(marker: impl Into<String>) -> Condition {
        Condition::Inline(format!("agent_output.contains('{}')", marker.into()))
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
mod tests {
    use super::patterns::*;
    use super::*;

    #[test]
    fn test_condition_builder_and() {
        let condition = ConditionBuilder::and()
            .inline("check1")
            .inline("check2")
            .build();

        match condition {
            Condition::Composite {
                operator,
                conditions,
            } => {
                assert_eq!(operator, LogicalOperator::And);
                assert_eq!(conditions.len(), 2);
            }
            _ => assert!(false, "Expected composite condition"),
        }
    }

    #[test]
    fn test_condition_builder_single() {
        let condition = ConditionBuilder::and().inline("only_one").build();

        match condition {
            Condition::Inline(expr) => assert_eq!(expr, "only_one"),
            _ => assert!(false, "Expected inline condition for single item"),
        }
    }

    #[tokio::test]
    async fn test_quick_check() {
        let ctx = GuardrailContext::new().with_agent_output("agent1", "test output");

        let result = quick_check("agent_output.contains('test')", &ctx)
            .await
            .unwrap();
        assert!(result.is_pass());

        let result = quick_check("agent_output.contains('missing')", &ctx)
            .await
            .unwrap();
        assert!(result.is_fail());
    }

    #[test]
    fn test_pattern_no_secrets() {
        let condition = no_secrets_in_output();

        match condition {
            Condition::Composite {
                operator,
                conditions,
            } => {
                assert_eq!(operator, LogicalOperator::And);
                assert!(conditions.len() >= 4);
            }
            _ => assert!(false, "Expected composite condition"),
        }
    }

    #[test]
    fn test_pattern_max_output_length() {
        let condition = max_output_length(1000);

        match condition {
            Condition::Inline(expr) => {
                assert!(expr.contains("1000"));
                assert!(expr.contains("agent_output.len()"));
            }
            _ => assert!(false, "Expected inline condition"),
        }
    }

    #[test]
    fn test_pattern_safe_shell() {
        let condition = safe_shell_command();

        match condition {
            Condition::Composite {
                operator,
                conditions,
            } => {
                assert_eq!(operator, LogicalOperator::And);
                assert!(conditions.len() >= 3);
            }
            _ => assert!(false, "Expected composite condition"),
        }
    }
}
