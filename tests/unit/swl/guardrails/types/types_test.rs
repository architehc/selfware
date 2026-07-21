use super::*;

#[test]
fn test_guardrail_type_from_str() {
    assert_eq!(
        GuardrailType::parse_str("pre_agent"),
        Some(GuardrailType::PreAgent)
    );
    assert_eq!(
        GuardrailType::parse_str("post_agent"),
        Some(GuardrailType::PostAgent)
    );
    assert_eq!(
        GuardrailType::parse_str("pre_tool"),
        Some(GuardrailType::PreTool)
    );
    assert_eq!(
        GuardrailType::parse_str("post_tool"),
        Some(GuardrailType::PostTool)
    );
    assert_eq!(GuardrailType::parse_str("unknown"), None);
}

#[test]
fn test_violation_action_from_str() {
    assert_eq!(
        ViolationAction::parse_str("block"),
        Some(ViolationAction::Block)
    );
    assert_eq!(
        ViolationAction::parse_str("warn"),
        Some(ViolationAction::Warn)
    );
    assert_eq!(
        ViolationAction::parse_str("log"),
        Some(ViolationAction::Log)
    );
    assert_eq!(
        ViolationAction::parse_str("alert"),
        Some(ViolationAction::Alert)
    );
    assert_eq!(ViolationAction::parse_str("unknown"), None);
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

    let fail = EvaluationResult::Fail {
        reason: "test".to_string(),
    };
    assert!(fail.is_fail());
    assert!(!fail.is_pass());

    let error = EvaluationResult::Error {
        message: "error".to_string(),
    };
    assert!(error.is_error());
}

#[test]
fn test_guardrail_summary() {
    let mut summary = GuardrailSummary {
        blocked: 1,
        ..Default::default()
    };
    summary.outcomes.push(GuardrailOutcome {
        guardrail_name: "test".to_string(),
        guardrail_type: GuardrailType::PreAgent,
        result: EvaluationResult::Fail {
            reason: "test".to_string(),
        },
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
