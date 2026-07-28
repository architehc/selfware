//! Guardrail Integration Tests
//!
//! Integration tests for the guardrail enforcement system.

use super::{enforcer::GuardrailEnforcer, engine::GuardrailEngine, types::*};
use crate::swl::parser::ast::{CodeBlock, CodeLanguage, GuardCondition, Guardrail};

/// Test helper to create a simple inline guardrail
fn create_inline_guardrail(
    name: &str,
    guardrail_type: GuardrailType,
    condition: &str,
    action: ViolationAction,
) -> GuardrailDef {
    GuardrailDef {
        name: name.to_string(),
        guardrail_type,
        condition: Condition::Inline(condition.to_string()),
        on_violation: action,
        description: None,
        severity: None,
        tags: Vec::new(),
    }
}

#[tokio::test]
async fn test_pre_agent_guardrail_blocks_execution() {
    let mut enforcer = GuardrailEnforcer::new();

    // Register a guardrail that always blocks
    enforcer.register_guardrail(create_inline_guardrail(
        "always_block",
        GuardrailType::PreAgent,
        "false",
        ViolationAction::Block,
    ));

    let ctx = GuardrailContext::new().with_current_agent("test_agent");

    let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

    assert!(summary.should_block());
    assert_eq!(summary.blocked, 1);
    assert_eq!(summary.failed, 1);
}

#[tokio::test]
async fn test_post_agent_guardrail_detects_critical_issues() {
    let mut enforcer = GuardrailEnforcer::new();

    // Register a guardrail that blocks on critical issues
    enforcer.register_guardrail(create_inline_guardrail(
        "block_critical",
        GuardrailType::PostAgent,
        "!agent_output.contains('[CRITICAL]')",
        ViolationAction::Block,
    ));

    // Test with critical output - should block
    let ctx = GuardrailContext::new()
        .with_agent_output("agent1", "Found [CRITICAL] security vulnerability");

    let summary = enforcer
        .check(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();

    assert!(summary.should_block());
    assert_eq!(summary.blocked, 1);

    // Test with safe output - should pass
    let ctx = GuardrailContext::new().with_agent_output("agent1", "All checks passed successfully");

    let summary = enforcer
        .check(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();

    assert!(!summary.should_block());
    assert_eq!(summary.passed, 1);
}

#[tokio::test]
async fn test_pre_tool_guardrail_blocks_dangerous_commands() {
    let mut enforcer = GuardrailEnforcer::new();

    // Register a guardrail that blocks rm -rf
    enforcer.register_guardrail(create_inline_guardrail(
        "no_rm_rf",
        GuardrailType::PreTool,
        "!tool_input.contains('rm -rf')",
        ViolationAction::Block,
    ));

    // Test with dangerous command - should block
    let ctx = GuardrailContext::new()
        .with_current_tool("shell")
        .with_tool_input("rm -rf /");

    let summary = enforcer.check(GuardrailType::PreTool, &ctx).await.unwrap();

    assert!(summary.should_block());

    // Test with safe command - should pass
    let ctx = GuardrailContext::new()
        .with_current_tool("shell")
        .with_tool_input("ls -la");

    let summary = enforcer.check(GuardrailType::PreTool, &ctx).await.unwrap();

    assert!(!summary.should_block());
}

#[tokio::test]
async fn test_warn_action_does_not_block() {
    let mut enforcer = GuardrailEnforcer::new();

    // Register a guardrail with warn action
    enforcer.register_guardrail(create_inline_guardrail(
        "warn_only",
        GuardrailType::PostAgent,
        "false",
        ViolationAction::Warn,
    ));

    let ctx = GuardrailContext::new().with_agent_output("agent1", "some output");

    let summary = enforcer
        .check(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();

    // Should fail but not block
    assert!(!summary.should_block());
    assert_eq!(summary.warnings, 1);
    assert_eq!(summary.failed, 1);
}

#[tokio::test]
async fn test_composite_and_conditions() {
    let engine = GuardrailEngine::new();

    let condition = Condition::Composite {
        operator: LogicalOperator::And,
        conditions: vec![
            Condition::Inline("true".to_string()),
            Condition::Inline("true".to_string()),
            Condition::Inline("true".to_string()),
        ],
    };

    let ctx = GuardrailContext::new();
    let result = engine.evaluate_condition(&condition, &ctx);

    assert!(result.is_pass());
}

#[tokio::test]
async fn test_composite_and_conditions_fail() {
    let engine = GuardrailEngine::new();

    let condition = Condition::Composite {
        operator: LogicalOperator::And,
        conditions: vec![
            Condition::Inline("true".to_string()),
            Condition::Inline("false".to_string()),
            Condition::Inline("true".to_string()),
        ],
    };

    let ctx = GuardrailContext::new();
    let result = engine.evaluate_condition(&condition, &ctx);

    assert!(result.is_fail());
}

#[tokio::test]
async fn test_composite_or_conditions() {
    let engine = GuardrailEngine::new();

    let condition = Condition::Composite {
        operator: LogicalOperator::Or,
        conditions: vec![
            Condition::Inline("false".to_string()),
            Condition::Inline("true".to_string()),
            Condition::Inline("false".to_string()),
        ],
    };

    let ctx = GuardrailContext::new();
    let result = engine.evaluate_condition(&condition, &ctx);

    assert!(result.is_pass());
}

#[tokio::test]
async fn test_composite_or_conditions_all_fail() {
    let engine = GuardrailEngine::new();

    let condition = Condition::Composite {
        operator: LogicalOperator::Or,
        conditions: vec![
            Condition::Inline("false".to_string()),
            Condition::Inline("false".to_string()),
        ],
    };

    let ctx = GuardrailContext::new();
    let result = engine.evaluate_condition(&condition, &ctx);

    assert!(result.is_fail());
}

#[tokio::test]
async fn test_state_based_conditions() {
    let engine = GuardrailEngine::new();

    let condition = Condition::Inline("state.count > 5".to_string());

    let ctx = GuardrailContext::new().with_state("count", 10);

    let result = engine.evaluate_condition(&condition, &ctx);
    assert!(result.is_pass());

    let ctx = GuardrailContext::new().with_state("count", 3);

    let result = engine.evaluate_condition(&condition, &ctx);
    assert!(result.is_fail());
}

#[tokio::test]
async fn test_no_secrets_in_output_guardrail() {
    let engine = GuardrailEngine::new();

    let condition = Condition::Composite {
        operator: LogicalOperator::And,
        conditions: vec![
            Condition::Inline("!agent_output.contains('password:')".to_string()),
            Condition::Inline("!agent_output.contains('api_key:')".to_string()),
            Condition::Inline("!agent_output.contains('secret:')".to_string()),
        ],
    };

    // Test with safe output — no secrets present, all checks pass
    let ctx = GuardrailContext::new().with_agent_output("agent1", "The configuration is valid.");

    let result = engine.evaluate_condition(&condition, &ctx);
    assert!(result.is_pass());

    // Test with secret in output — api_key: present, check fails
    let ctx = GuardrailContext::new()
        .with_agent_output("agent1", "The api_key: sk-abc123 is configured.");

    let result = engine.evaluate_condition(&condition, &ctx);
    assert!(result.is_fail());
}

#[tokio::test]
async fn test_regex_pattern_matching() {
    let engine = GuardrailEngine::new();

    // Test with regex language code block
    let condition = Condition::Code {
        language: "regex".to_string(),
        content: r"\[CRITICAL\]|\[HIGH\]".to_string(),
    };

    let ctx = GuardrailContext::new().with_agent_output("agent1", "Found [HIGH] priority issue");

    let result = engine.evaluate_condition(&condition, &ctx);
    assert!(result.is_pass()); // Regex matches, so condition passes

    let ctx = GuardrailContext::new().with_agent_output("agent1", "All checks passed");

    let result = engine.evaluate_condition(&condition, &ctx);
    assert!(result.is_fail()); // Regex doesn't match, so condition fails
}

#[tokio::test]
async fn test_enforcer_from_ast_guardrails() {
    let mut enforcer = GuardrailEnforcer::new();

    let ast_guardrails = vec![Guardrail {
        name: Some("test_guardrail".to_string()),
        guardrail_type: Some("post_agent".to_string()),
        condition: GuardCondition::Inline("!agent_output.contains('ERROR')".to_string()),
        on_violation: "block".to_string(),
    }];

    enforcer.register_guardrails(&ast_guardrails);

    let ctx = GuardrailContext::new().with_agent_output("agent1", "Something ERROR happened");

    let summary = enforcer
        .check(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();

    assert!(summary.should_block());
}

#[tokio::test]
async fn test_guardrail_telemetry_collection() {
    let mut enforcer = GuardrailEnforcer::new();

    enforcer.register_guardrail(create_inline_guardrail(
        "telemetry_test",
        GuardrailType::PreAgent,
        "true",
        ViolationAction::Log,
    ));

    let ctx = GuardrailContext::new().with_current_agent("test_agent");

    enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

    let telemetry = enforcer.get_telemetry_events().await;
    assert_eq!(telemetry.len(), 1);
    assert_eq!(telemetry[0].guardrail_name, "telemetry_test");
    assert_eq!(telemetry[0].result, "pass");
}

#[tokio::test]
async fn test_multiple_guardrail_types() {
    let mut enforcer = GuardrailEnforcer::new();

    // Register guardrails for different types
    enforcer.register_guardrail(create_inline_guardrail(
        "pre_workflow_check",
        GuardrailType::PreWorkflow,
        "true",
        ViolationAction::Log,
    ));

    enforcer.register_guardrail(create_inline_guardrail(
        "pre_agent_check",
        GuardrailType::PreAgent,
        "true",
        ViolationAction::Log,
    ));

    enforcer.register_guardrail(create_inline_guardrail(
        "post_agent_check",
        GuardrailType::PostAgent,
        "true",
        ViolationAction::Log,
    ));

    let ctx = GuardrailContext::new();

    let pre_workflow_summary = enforcer
        .check(GuardrailType::PreWorkflow, &ctx)
        .await
        .unwrap();
    assert_eq!(pre_workflow_summary.total_checked, 1);

    let pre_agent_summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();
    assert_eq!(pre_agent_summary.total_checked, 1);

    let post_agent_summary = enforcer
        .check(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();
    assert_eq!(post_agent_summary.total_checked, 1);
}

#[test]
fn test_guardrail_context_json_conversion() {
    let ctx = GuardrailContext::new()
        .with_state("count", 42)
        .with_state("name", "test")
        .with_current_agent("my_agent")
        .with_agent_output("my_agent", "output data")
        .with_workflow_input("prompt", "test prompt");

    let json = ctx.to_json();

    assert!(json.get("state").is_some());
    assert!(json.get("current_agent").is_some());
    assert!(json.get("agent_output").is_some());
    assert!(json.get("workflow_inputs").is_some());
    assert!(json.get("agent_outputs").is_some());

    let state = json.get("state").unwrap();
    assert_eq!(state.get("count").unwrap().as_i64(), Some(42));
    assert_eq!(state.get("name").unwrap().as_str(), Some("test"));
}
