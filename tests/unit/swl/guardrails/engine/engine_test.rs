use super::*;

#[test]
fn test_evaluate_simple_boolean() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new();

    assert!(engine.evaluate_inline_expression("true", &ctx).is_pass());
    assert!(engine.evaluate_inline_expression("false", &ctx).is_fail());
}

#[test]
fn test_evaluate_contains() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new()
        .with_agent_output("agent1", "This is a test output with [CRITICAL] issue");

    let result = engine.evaluate_inline_expression("agent_output.contains('[CRITICAL]')", &ctx);
    assert!(result.is_pass(), "Should detect CRITICAL in output");

    let result = engine.evaluate_inline_expression("agent_output.contains('[LOW]')", &ctx);
    assert!(result.is_fail(), "Should not detect LOW in output");
}

#[test]
fn test_evaluate_negation() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_agent_output("agent1", "safe output");

    let result = engine.evaluate_inline_expression("!agent_output.contains('dangerous')", &ctx);
    assert!(result.is_pass(), "Negation should work");
}

#[test]
fn test_evaluate_comparison() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_state("count", 10);

    let result = engine.evaluate_inline_expression("state.count > 5", &ctx);
    assert!(result.is_pass(), "10 > 5 should pass");

    let result = engine.evaluate_inline_expression("state.count < 5", &ctx);
    assert!(result.is_fail(), "10 < 5 should fail");
}

#[test]
fn test_composite_and() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new()
        .with_agent_output("agent1", "test output")
        .with_state("count", 5);

    let conditions = vec![
        Condition::Inline("agent_output.contains('test')".to_string()),
        Condition::Inline("state.count > 3".to_string()),
    ];

    let result = engine.evaluate_composite_condition(LogicalOperator::And, &conditions, &ctx);
    assert!(result.is_pass());
}

#[test]
fn test_composite_or() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_agent_output("agent1", "test output");

    let conditions = vec![
        Condition::Inline("agent_output.contains('missing')".to_string()),
        Condition::Inline("agent_output.contains('test')".to_string()),
    ];

    let result = engine.evaluate_composite_condition(LogicalOperator::Or, &conditions, &ctx);
    assert!(result.is_pass());
}

#[test]
fn test_evaluate_rust_code() {
    let engine = GuardrailEngine::new();
    let ctx =
        GuardrailContext::new().with_agent_output("agent1", "[CRITICAL] Security issue found");

    // Use a direct inline expression that the simplified evaluator can handle.
    // The evaluator resolves agent_output from context and checks .contains().
    let code = r#"
            // Check for critical issues
            !agent_output.contains("[CRITICAL]")
        "#;

    let result = engine.evaluate_rust_code(code, &ctx);
    assert!(
        result.is_fail(),
        "Should detect CRITICAL in code evaluation"
    );
}

#[test]
fn test_evaluate_equality() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_state("count", 42);

    // Test numeric equality
    let result = engine.evaluate_inline_expression("state.count == 42", &ctx);
    assert!(
        result.is_pass(),
        "state.count == 42 should pass, got: {:?}",
        result
    );

    // Test inequality
    let result = engine.evaluate_inline_expression("state.count != 10", &ctx);
    assert!(
        result.is_pass(),
        "state.count != 10 should pass, got: {:?}",
        result
    );

    // Test string equality
    let ctx2 = GuardrailContext::new().with_state("name", "test");
    let result = engine.evaluate_inline_expression("state.name == 'test'", &ctx2);
    assert!(
        result.is_pass(),
        "state.name == 'test' should pass, got: {:?}",
        result
    );
}

#[test]
fn test_json_logic_comparison_operators() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_state("count", 10);

    // Test >= operator
    let json_logic = r#"{">=": [{"var": "count"}, 5]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(
        result.is_pass(),
        ">= should pass when count is 10, got: {:?}",
        result
    );

    // Test < operator
    let json_logic = r#"{"<": [{"var": "count"}, 20]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(
        result.is_pass(),
        "< should pass when count is 10 and comparing to 20, got: {:?}",
        result
    );
}

#[test]
fn test_json_logic_and_operator() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new()
        .with_state("count", 10)
        .with_state("enabled", true);

    // Test AND with two true conditions
    let json_logic = r#"{"and": [{"var": "count"}, {"var": "enabled"}]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(
        result.is_pass(),
        "AND should pass when both conditions are true, got: {:?}",
        result
    );
}
