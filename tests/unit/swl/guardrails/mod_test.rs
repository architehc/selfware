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
        _ => unreachable!("Expected composite condition"),
    }
}

#[test]
fn test_condition_builder_single() {
    let condition = ConditionBuilder::and().inline("only_one").build();

    match condition {
        Condition::Inline(expr) => assert_eq!(expr, "only_one"),
        _ => unreachable!("Expected inline condition for single item"),
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
        _ => unreachable!("Expected composite condition"),
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
        _ => unreachable!("Expected inline condition"),
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
        _ => unreachable!("Expected composite condition"),
    }
}
