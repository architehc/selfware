//! Guardrail Types
//!
//! Core types for guardrail enforcement in SWL workflows.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type of guardrail trigger point
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GuardrailType {
    /// Check before agent execution
    #[serde(rename = "pre_agent")]
    PreAgent,
    /// Check after agent execution
    #[serde(rename = "post_agent")]
    PostAgent,
    /// Check before tool execution
    #[serde(rename = "pre_tool")]
    PreTool,
    /// Check after tool execution
    #[serde(rename = "post_tool")]
    PostTool,
    /// Check before workflow execution
    #[serde(rename = "pre_workflow")]
    PreWorkflow,
    /// Check after workflow execution
    #[serde(rename = "post_workflow")]
    PostWorkflow,
}

impl std::fmt::Display for GuardrailType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardrailType::PreAgent => write!(f, "pre_agent"),
            GuardrailType::PostAgent => write!(f, "post_agent"),
            GuardrailType::PreTool => write!(f, "pre_tool"),
            GuardrailType::PostTool => write!(f, "post_tool"),
            GuardrailType::PreWorkflow => write!(f, "pre_workflow"),
            GuardrailType::PostWorkflow => write!(f, "post_workflow"),
        }
    }
}

impl GuardrailType {
    /// Parse from string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pre_agent" => Some(GuardrailType::PreAgent),
            "post_agent" => Some(GuardrailType::PostAgent),
            "pre_tool" => Some(GuardrailType::PreTool),
            "post_tool" => Some(GuardrailType::PostTool),
            "pre_workflow" => Some(GuardrailType::PreWorkflow),
            "post_workflow" => Some(GuardrailType::PostWorkflow),
            _ => None,
        }
    }

    /// All guardrail types
    pub fn all() -> Vec<Self> {
        vec![
            GuardrailType::PreAgent,
            GuardrailType::PostAgent,
            GuardrailType::PreTool,
            GuardrailType::PostTool,
            GuardrailType::PreWorkflow,
            GuardrailType::PostWorkflow,
        ]
    }
}

/// Action to take when guardrail condition is violated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViolationAction {
    /// Block the operation and fail
    #[serde(rename = "block")]
    Block,
    /// Log warning but continue
    #[serde(rename = "warn")]
    Warn,
    /// Log silently
    #[serde(rename = "log")]
    Log,
    /// Alert (notify external system)
    #[serde(rename = "alert")]
    Alert,
}

impl std::fmt::Display for ViolationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolationAction::Block => write!(f, "block"),
            ViolationAction::Warn => write!(f, "warn"),
            ViolationAction::Log => write!(f, "log"),
            ViolationAction::Alert => write!(f, "alert"),
        }
    }
}

impl ViolationAction {
    /// Parse from string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "block" => Some(ViolationAction::Block),
            "warn" => Some(ViolationAction::Warn),
            "log" => Some(ViolationAction::Log),
            "alert" => Some(ViolationAction::Alert),
            _ => None,
        }
    }
}

/// Logical operator for combining conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicalOperator {
    /// All conditions must be true
    #[serde(rename = "and")]
    And,
    /// At least one condition must be true
    #[serde(rename = "or")]
    Or,
}

impl Default for LogicalOperator {
    fn default() -> Self {
        LogicalOperator::And
    }
}

/// A condition that can be evaluated
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    /// Simple inline expression
    Inline(String),
    /// Code block in a specific language
    Code {
        language: String,
        content: String,
    },
    /// Composite condition with logical operator
    Composite {
        operator: LogicalOperator,
        conditions: Vec<Condition>,
    },
}

/// Guardrail definition for enforcement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardrailDef {
    /// Unique name for the guardrail
    pub name: String,
    /// Type of trigger point
    #[serde(rename = "type")]
    pub guardrail_type: GuardrailType,
    /// Condition to evaluate
    pub condition: Condition,
    /// Action to take on violation
    pub on_violation: ViolationAction,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional severity level
    #[serde(default)]
    pub severity: Option<GuardrailSeverity>,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Severity level for guardrail violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GuardrailSeverity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "critical")]
    Critical,
}

impl Default for GuardrailSeverity {
    fn default() -> Self {
        GuardrailSeverity::Medium
    }
}

impl std::fmt::Display for GuardrailSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardrailSeverity::Info => write!(f, "info"),
            GuardrailSeverity::Low => write!(f, "low"),
            GuardrailSeverity::Medium => write!(f, "medium"),
            GuardrailSeverity::High => write!(f, "high"),
            GuardrailSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Context for evaluating guardrail conditions
#[derive(Debug, Clone, Default)]
pub struct GuardrailContext {
    /// Current state values
    pub state: HashMap<String, serde_json::Value>,
    /// Agent outputs (for post-agent checks)
    pub agent_outputs: HashMap<String, String>,
    /// Current agent name (if applicable)
    pub current_agent: Option<String>,
    /// Current tool name (if applicable)
    pub current_tool: Option<String>,
    /// Tool input arguments (for pre-tool checks)
    pub tool_input: Option<String>,
    /// Tool output (for post-tool checks)
    pub tool_output: Option<String>,
    /// Workflow inputs
    pub workflow_inputs: HashMap<String, serde_json::Value>,
    /// Current agent output (for post-agent checks)
    pub agent_output: Option<String>,
}

impl GuardrailContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add state value
    pub fn with_state(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.state.insert(key.into(), value.into());
        self
    }

    /// Add agent output
    pub fn with_agent_output(mut self, agent: impl Into<String>, output: impl Into<String>) -> Self {
        let agent_name = agent.into();
        let output_str = output.into();
        self.agent_outputs.insert(agent_name.clone(), output_str.clone());
        self.current_agent = Some(agent_name);
        self.agent_output = Some(output_str);
        self
    }

    /// Set current agent
    pub fn with_current_agent(mut self, agent: impl Into<String>) -> Self {
        self.current_agent = Some(agent.into());
        self
    }

    /// Set current tool
    pub fn with_current_tool(mut self, tool: impl Into<String>) -> Self {
        self.current_tool = Some(tool.into());
        self
    }

    /// Set tool input
    pub fn with_tool_input(mut self, input: impl Into<String>) -> Self {
        self.tool_input = Some(input.into());
        self
    }

    /// Set tool output
    pub fn with_tool_output(mut self, output: impl Into<String>) -> Self {
        self.tool_output = Some(output.into());
        self
    }

    /// Add workflow input
    pub fn with_workflow_input(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.workflow_inputs.insert(key.into(), value.into());
        self
    }

    /// Convert to JSON for condition evaluation
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        
        map.insert("state".to_string(), serde_json::to_value(&self.state).unwrap_or_default());
        map.insert("agent_outputs".to_string(), serde_json::to_value(&self.agent_outputs).unwrap_or_default());
        map.insert("workflow_inputs".to_string(), serde_json::to_value(&self.workflow_inputs).unwrap_or_default());
        
        if let Some(agent) = &self.current_agent {
            map.insert("current_agent".to_string(), agent.clone().into());
        }
        if let Some(tool) = &self.current_tool {
            map.insert("current_tool".to_string(), tool.clone().into());
        }
        if let Some(input) = &self.tool_input {
            map.insert("tool_input".to_string(), input.clone().into());
        }
        if let Some(output) = &self.tool_output {
            map.insert("tool_output".to_string(), output.clone().into());
        }
        if let Some(output) = &self.agent_output {
            map.insert("agent_output".to_string(), output.clone().into());
        }
        
        serde_json::Value::Object(map)
    }
}

/// Result of guardrail evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationResult {
    /// Condition passed (no violation)
    Pass,
    /// Condition failed (violation detected)
    Fail { reason: String },
    /// Evaluation could not be completed
    Error { message: String },
}

impl EvaluationResult {
    /// Check if the evaluation passed
    pub fn is_pass(&self) -> bool {
        matches!(self, EvaluationResult::Pass)
    }

    /// Check if the evaluation failed
    pub fn is_fail(&self) -> bool {
        matches!(self, EvaluationResult::Fail { .. })
    }

    /// Check if there was an error
    pub fn is_error(&self) -> bool {
        matches!(self, EvaluationResult::Error { .. })
    }
}

/// Outcome of checking a guardrail
#[derive(Debug, Clone)]
pub struct GuardrailOutcome {
    /// Name of the guardrail
    pub guardrail_name: String,
    /// Type of guardrail
    pub guardrail_type: GuardrailType,
    /// Evaluation result
    pub result: EvaluationResult,
    /// Action that was or should be taken
    pub action: ViolationAction,
    /// Timestamp of evaluation
    pub timestamp: std::time::Instant,
    /// Duration of evaluation
    pub evaluation_duration_ms: u64,
}

/// Summary of guardrail check results
#[derive(Debug, Clone, Default)]
pub struct GuardrailSummary {
    /// Total number of guardrails checked
    pub total_checked: usize,
    /// Number of passed checks
    pub passed: usize,
    /// Number of failed checks (violations)
    pub failed: usize,
    /// Number of evaluation errors
    pub errors: usize,
    /// Number of blocked operations
    pub blocked: usize,
    /// Number of warnings issued
    pub warnings: usize,
    /// Detailed outcomes
    pub outcomes: Vec<GuardrailOutcome>,
}

impl GuardrailSummary {
    /// Check if any violations should block execution
    pub fn should_block(&self) -> bool {
        self.outcomes.iter().any(|o| {
            matches!(o.action, ViolationAction::Block) && o.result.is_fail()
        })
    }

    /// Get all blocking violations
    pub fn blocking_violations(&self) -> Vec<&GuardrailOutcome> {
        self.outcomes
            .iter()
            .filter(|o| matches!(o.action, ViolationAction::Block) && o.result.is_fail())
            .collect()
    }

    /// Get all warnings
    pub fn warnings(&self) -> Vec<&GuardrailOutcome> {
        self.outcomes
            .iter()
            .filter(|o| matches!(o.action, ViolationAction::Warn) && o.result.is_fail())
            .collect()
    }
}

/// Telemetry event for guardrail evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailTelemetryEvent {
    /// Event timestamp (ISO 8601)
    pub timestamp: String,
    /// Guardrail name
    pub guardrail_name: String,
    /// Guardrail type
    pub guardrail_type: String,
    /// Evaluation result (pass/fail/error)
    pub result: String,
    /// Action taken
    pub action: String,
    /// Evaluation duration in milliseconds
    pub duration_ms: u64,
    /// Optional error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Optional failure reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Current agent (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Current tool (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Workflow name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guardrail_type_from_str() {
        assert_eq!(GuardrailType::from_str("pre_agent"), Some(GuardrailType::PreAgent));
        assert_eq!(GuardrailType::from_str("post_agent"), Some(GuardrailType::PostAgent));
        assert_eq!(GuardrailType::from_str("pre_tool"), Some(GuardrailType::PreTool));
        assert_eq!(GuardrailType::from_str("post_tool"), Some(GuardrailType::PostTool));
        assert_eq!(GuardrailType::from_str("unknown"), None);
    }

    #[test]
    fn test_violation_action_from_str() {
        assert_eq!(ViolationAction::from_str("block"), Some(ViolationAction::Block));
        assert_eq!(ViolationAction::from_str("warn"), Some(ViolationAction::Warn));
        assert_eq!(ViolationAction::from_str("log"), Some(ViolationAction::Log));
        assert_eq!(ViolationAction::from_str("alert"), Some(ViolationAction::Alert));
        assert_eq!(ViolationAction::from_str("unknown"), None);
    }

    #[test]
    fn test_guardrail_context_builder() {
        let ctx = GuardrailContext::new()
            .with_state("key", "value")
            .with_current_agent("test_agent")
            .with_agent_output("test_agent", "output content");

        assert_eq!(ctx.current_agent, Some("test_agent".to_string()));
        assert_eq!(ctx.agent_output, Some("output content".to_string()));
        assert!(ctx.state.contains_key("key"));
    }

    #[test]
    fn test_evaluation_result() {
        assert!(EvaluationResult::Pass.is_pass());
        assert!(!EvaluationResult::Pass.is_fail());
        
        let fail = EvaluationResult::Fail { reason: "test".to_string() };
        assert!(fail.is_fail());
        assert!(!fail.is_pass());
        
        let error = EvaluationResult::Error { message: "error".to_string() };
        assert!(error.is_error());
    }

    #[test]
    fn test_guardrail_summary() {
        let mut summary = GuardrailSummary::default();
        summary.blocked = 1;
        summary.outcomes.push(GuardrailOutcome {
            guardrail_name: "test".to_string(),
            guardrail_type: GuardrailType::PreAgent,
            result: EvaluationResult::Fail { reason: "test".to_string() },
            action: ViolationAction::Block,
            timestamp: std::time::Instant::now(),
            evaluation_duration_ms: 10,
        });

        assert!(summary.should_block());
        assert_eq!(summary.blocking_violations().len(), 1);
    }

    #[test]
    fn test_guardrail_severity_ordering() {
        assert!(GuardrailSeverity::Critical > GuardrailSeverity::High);
        assert!(GuardrailSeverity::High > GuardrailSeverity::Medium);
        assert!(GuardrailSeverity::Medium > GuardrailSeverity::Low);
        assert!(GuardrailSeverity::Low > GuardrailSeverity::Info);
    }

    #[test]
    fn test_guardrail_context_to_json() {
        let ctx = GuardrailContext::new()
            .with_state("count", 42)
            .with_current_agent("agent1")
            .with_agent_output("agent1", "result");

        let json = ctx.to_json();
        assert!(json.get("state").is_some());
        assert!(json.get("current_agent").is_some());
        assert!(json.get("agent_output").is_some());
    }
}
