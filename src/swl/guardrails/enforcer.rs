//! Guardrail Enforcer
//!
//! Enforces guardrails at various execution checkpoints during workflow execution.

use super::engine::GuardrailEngine;
use super::types::*;
use crate::errors::{SafetyError, SelfwareError};
use crate::observability::telemetry;
use crate::swl::parser::ast::{GuardCondition, Guardrail};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Enforces guardrails during workflow execution
pub struct GuardrailEnforcer {
    /// Engine for evaluating conditions
    engine: GuardrailEngine,
    /// Active guardrails by type
    guardrails: HashMap<GuardrailType, Vec<GuardrailDef>>,
    /// Telemetry events
    telemetry_events: Arc<Mutex<Vec<GuardrailTelemetryEvent>>>,
    /// Enable verbose logging
    verbose: bool,
}

impl GuardrailEnforcer {
    /// Create a new guardrail enforcer
    pub fn new() -> Self {
        Self {
            engine: GuardrailEngine::new(),
            guardrails: HashMap::new(),
            telemetry_events: Arc::new(Mutex::new(Vec::new())),
            verbose: false,
        }
    }

    /// Create with verbose logging
    pub fn new_verbose() -> Self {
        Self {
            engine: GuardrailEngine::new_verbose(),
            guardrails: HashMap::new(),
            telemetry_events: Arc::new(Mutex::new(Vec::new())),
            verbose: true,
        }
    }

    /// Register guardrails from SWL document
    pub fn register_guardrails(&mut self, guardrails: &[Guardrail]) {
        for guardrail in guardrails {
            if let Some(guardrail_type) = guardrail
                .guardrail_type
                .as_ref()
                .and_then(|t| GuardrailType::from_str(t))
            {
                let def = GuardrailDef {
                    name: guardrail.name.clone().unwrap_or_else(|| "unnamed".to_string()),
                    guardrail_type,
                    condition: self.convert_condition(&guardrail.condition),
                    on_violation: ViolationAction::from_str(&guardrail.on_violation)
                        .unwrap_or(ViolationAction::Log),
                    description: None,
                    severity: None,
                    tags: Vec::new(),
                };

                self.guardrails
                    .entry(guardrail_type)
                    .or_default()
                    .push(def);

                if self.verbose {
                    info!(
                        "Registered guardrail: {} ({})",
                        def.name, def.guardrail_type
                    );
                }
            }
        }
    }

    /// Register a single guardrail definition
    pub fn register_guardrail(&mut self, guardrail: GuardrailDef) {
        let guardrail_type = guardrail.guardrail_type;
        self.guardrails
            .entry(guardrail_type)
            .or_default()
            .push(guardrail);
    }

    /// Check guardrails of a specific type
    pub async fn check(
        &self,
        guardrail_type: GuardrailType,
        context: &GuardrailContext,
    ) -> Result<GuardrailSummary, SelfwareError> {
        let start = std::time::Instant::now();
        let mut summary = GuardrailSummary::default();

        let guardrails = self.guardrails.get(&guardrail_type).cloned().unwrap_or_default();
        
        if guardrails.is_empty() {
            return Ok(summary);
        }

        debug!(
            "Checking {} guardrails for type: {:?}",
            guardrails.len(),
            guardrail_type
        );

        for guardrail in &guardrails {
            let outcome = self.evaluate_guardrail(guardrail, context).await;
            
            // Update summary
            summary.total_checked += 1;
            match &outcome.result {
                EvaluationResult::Pass => summary.passed += 1,
                EvaluationResult::Fail { .. } => {
                    summary.failed += 1;
                    match outcome.action {
                        ViolationAction::Block => summary.blocked += 1,
                        ViolationAction::Warn => summary.warnings += 1,
                        _ => {}
                    }
                }
                EvaluationResult::Error { .. } => summary.errors += 1,
            }
            
            summary.outcomes.push(outcome);
        }

        let total_duration_ms = start.elapsed().as_millis() as u64;
        debug!(
            "Guardrail check completed in {}ms: {} passed, {} failed, {} errors",
            total_duration_ms, summary.passed, summary.failed, summary.errors
        );

        // Emit telemetry
        self.emit_telemetry(&summary, guardrail_type, context).await;
        
        // Record metrics
        increment_guardrail_checks(summary.total_checked as u64);
        increment_guardrail_violations(summary.failed as u64);

        Ok(summary)
    }

    /// Check if execution should be blocked
    pub async fn should_block(
        &self,
        guardrail_type: GuardrailType,
        context: &GuardrailContext,
    ) -> Result<Option<Vec<GuardrailOutcome>>, SelfwareError> {
        let summary = self.check(guardrail_type, context).await?;
        
        if summary.should_block() {
            Ok(Some(summary.outcomes))
        } else {
            Ok(None)
        }
    }

    /// Evaluate a single guardrail
    async fn evaluate_guardrail(
        &self,
        guardrail: &GuardrailDef,
        context: &GuardrailContext,
    ) -> GuardrailOutcome {
        let start = std::time::Instant::now();
        
        let result = self.engine.evaluate_condition(&guardrail.condition, context);
        
        let duration_ms = start.elapsed().as_millis() as u64;

        // Handle the action based on result
        if result.is_fail() {
            match guardrail.on_violation {
                ViolationAction::Block => {
                    warn!(
                        guardrail = %guardrail.name,
                        guardrail_type = %guardrail.guardrail_type,
                        "Guardrail violation - BLOCKING execution"
                    );
                }
                ViolationAction::Warn => {
                    warn!(
                        guardrail = %guardrail.name,
                        guardrail_type = %guardrail.guardrail_type,
                        "Guardrail violation - WARNING issued"
                    );
                }
                ViolationAction::Log => {
                    info!(
                        guardrail = %guardrail.name,
                        guardrail_type = %guardrail.guardrail_type,
                        "Guardrail violation - logged"
                    );
                }
                ViolationAction::Alert => {
                    warn!(
                        guardrail = %guardrail.name,
                        guardrail_type = %guardrail.guardrail_type,
                        "Guardrail violation - ALERT triggered"
                    );
                }
            }
        } else if self.verbose {
            debug!(
                guardrail = %guardrail.name,
                guardrail_type = %guardrail.guardrail_type,
                "Guardrail check passed"
            );
        }

        GuardrailOutcome {
            guardrail_name: guardrail.name.clone(),
            guardrail_type: guardrail.guardrail_type,
            result,
            action: guardrail.on_violation,
            timestamp: start,
            evaluation_duration_ms: duration_ms,
        }
    }

    /// Convert AST condition to engine condition
    fn convert_condition(&self, condition: &GuardCondition) -> Condition {
        match condition {
            GuardCondition::Inline(expr) => Condition::Inline(expr.clone()),
            GuardCondition::Code(block) => Condition::Code {
                language: block.language.to_string().to_lowercase(),
                content: block.code.clone(),
            },
        }
    }

    /// Emit telemetry events for guardrail evaluations
    async fn emit_telemetry(
        &self,
        summary: &GuardrailSummary,
        guardrail_type: GuardrailType,
        context: &GuardrailContext,
    ) {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let workflow_name = context.state.get("workflow_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let events: Vec<GuardrailTelemetryEvent> = summary
            .outcomes
            .iter()
            .map(|outcome| GuardrailTelemetryEvent {
                timestamp: timestamp.clone(),
                guardrail_name: outcome.guardrail_name.clone(),
                guardrail_type: outcome.guardrail_type.to_string(),
                result: match &outcome.result {
                    EvaluationResult::Pass => "pass".to_string(),
                    EvaluationResult::Fail { .. } => "fail".to_string(),
                    EvaluationResult::Error { .. } => "error".to_string(),
                },
                action: outcome.action.to_string(),
                duration_ms: outcome.evaluation_duration_ms,
                error_message: match &outcome.result {
                    EvaluationResult::Error { message } => Some(message.clone()),
                    _ => None,
                },
                failure_reason: match &outcome.result {
                    EvaluationResult::Fail { reason } => Some(reason.clone()),
                    _ => None,
                },
                agent_name: context.current_agent.clone(),
                tool_name: context.current_tool.clone(),
                workflow_name: workflow_name.clone(),
            })
            .collect();

        // Store events
        if let Ok(mut stored) = self.telemetry_events.lock().await {
            stored.extend(events.clone());
        }

    }

    /// Get all telemetry events
    pub async fn get_telemetry_events(&self) -> Vec<GuardrailTelemetryEvent> {
        self.telemetry_events.lock().await.clone()
    }

    /// Clear telemetry events
    pub async fn clear_telemetry(&self) {
        self.telemetry_events.lock().await.clear();
    }

    /// Export telemetry as JSON
    pub async fn export_telemetry_json(&self) -> Result<String, SelfwareError> {
        let events = self.get_telemetry_events().await;
        serde_json::to_string_pretty(&events)
            .map_err(|e| SelfwareError::Internal(format!("Failed to serialize telemetry: {}", e)))
    }

    /// Get guardrail statistics
    pub fn get_stats(&self) -> GuardrailStats {
        let total = self.guardrails.values().map(|v| v.len()).sum();
        let by_type: HashMap<String, usize> = self
            .guardrails
            .iter()
            .map(|(k, v)| (k.to_string(), v.len()))
            .collect();

        GuardrailStats {
            total_guardrails: total,
            by_type,
        }
    }
}

impl Default for GuardrailEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about registered guardrails
#[derive(Debug, Clone)]
pub struct GuardrailStats {
    /// Total number of guardrails
    pub total_guardrails: usize,
    /// Count by guardrail type
    pub by_type: HashMap<String, usize>,
}

/// Helper functions for guardrail telemetry
fn increment_guardrail_checks(count: u64) {
    metrics::counter!("swl_guardrail_checks_total", count);
}

fn increment_guardrail_violations(count: u64) {
    metrics::counter!("swl_guardrail_violations_total", count);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_guardrail(name: &str, guardrail_type: GuardrailType, condition: &str) -> GuardrailDef {
        GuardrailDef {
            name: name.to_string(),
            guardrail_type,
            condition: Condition::Inline(condition.to_string()),
            on_violation: ViolationAction::Block,
            description: None,
            severity: None,
            tags: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_enforcer_check_pass() {
        let mut enforcer = GuardrailEnforcer::new();
        enforcer.register_guardrail(create_test_guardrail(
            "test_pass",
            GuardrailType::PreAgent,
            "true",
        ));

        let ctx = GuardrailContext::new();
        let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

        assert_eq!(summary.total_checked, 1);
        assert_eq!(summary.passed, 1);
        assert!(!summary.should_block());
    }

    #[tokio::test]
    async fn test_enforcer_check_fail() {
        let mut enforcer = GuardrailEnforcer::new();
        enforcer.register_guardrail(create_test_guardrail(
            "test_fail",
            GuardrailType::PreAgent,
            "false",
        ));

        let ctx = GuardrailContext::new();
        let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

        assert_eq!(summary.total_checked, 1);
        assert_eq!(summary.failed, 1);
        assert!(summary.should_block());
    }

    #[tokio::test]
    async fn test_enforcer_no_guardrails() {
        let enforcer = GuardrailEnforcer::new();
        let ctx = GuardrailContext::new();
        let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

        assert_eq!(summary.total_checked, 0);
        assert!(!summary.should_block());
    }

    #[tokio::test]
    async fn test_enforcer_stats() {
        let mut enforcer = GuardrailEnforcer::new();
        enforcer.register_guardrail(create_test_guardrail(
            "g1",
            GuardrailType::PreAgent,
            "true",
        ));
        enforcer.register_guardrail(create_test_guardrail(
            "g2",
            GuardrailType::PostAgent,
            "true",
        ));
        enforcer.register_guardrail(create_test_guardrail(
            "g3",
            GuardrailType::PreAgent,
            "true",
        ));

        let stats = enforcer.get_stats();
        assert_eq!(stats.total_guardrails, 3);
        assert_eq!(stats.by_type.get("pre_agent"), Some(&2));
        assert_eq!(stats.by_type.get("post_agent"), Some(&1));
    }

    #[tokio::test]
    async fn test_enforcer_should_block() {
        let mut enforcer = GuardrailEnforcer::new();
        enforcer.register_guardrail(GuardrailDef {
            name: "block_on_critical".to_string(),
            guardrail_type: GuardrailType::PostAgent,
            condition: Condition::Inline("agent_output.contains('[CRITICAL]')".to_string()),
            on_violation: ViolationAction::Block,
            description: None,
            severity: None,
            tags: Vec::new(),
        });

        // Test with critical output - should block
        let ctx = GuardrailContext::new()
            .with_agent_output("agent1", "Found [CRITICAL] security issue");
        
        let blocking = enforcer.should_block(GuardrailType::PostAgent, &ctx).await.unwrap();
        assert!(blocking.is_some());

        // Test with safe output - should not block
        let ctx = GuardrailContext::new()
            .with_agent_output("agent1", "All checks passed");
        
        let blocking = enforcer.should_block(GuardrailType::PostAgent, &ctx).await.unwrap();
        assert!(blocking.is_none());
    }
}
