use super::*;

fn create_test_guardrail(
    name: &str,
    guardrail_type: GuardrailType,
    condition: &str,
) -> GuardrailDef {
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
    enforcer.register_guardrail(create_test_guardrail("g1", GuardrailType::PreAgent, "true"));
    enforcer.register_guardrail(create_test_guardrail(
        "g2",
        GuardrailType::PostAgent,
        "true",
    ));
    enforcer.register_guardrail(create_test_guardrail("g3", GuardrailType::PreAgent, "true"));

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
        // Condition: output should NOT contain CRITICAL. When it does, check fails → block.
        condition: Condition::Inline("!agent_output.contains('[CRITICAL]')".to_string()),
        on_violation: ViolationAction::Block,
        description: None,
        severity: None,
        tags: Vec::new(),
    });

    // Test with critical output - should block
    let ctx =
        GuardrailContext::new().with_agent_output("agent1", "Found [CRITICAL] security issue");

    let blocking = enforcer
        .should_block(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();
    assert!(blocking.is_some());

    // Test with safe output - should not block
    let ctx = GuardrailContext::new().with_agent_output("agent1", "All checks passed");

    let blocking = enforcer
        .should_block(GuardrailType::PostAgent, &ctx)
        .await
        .unwrap();
    assert!(blocking.is_none());
}
