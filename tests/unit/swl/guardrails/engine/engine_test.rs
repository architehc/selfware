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

#[test]
fn test_summary_should_block_for_fail_and_error_outcomes() {
    let fail_outcome = GuardrailOutcome {
        guardrail_name: "block_on_fail".to_string(),
        guardrail_type: GuardrailType::PreTool,
        result: EvaluationResult::Fail {
            reason: "blocked by policy".to_string(),
        },
        action: ViolationAction::Block,
        timestamp: std::time::Instant::now(),
        evaluation_duration_ms: 1,
    };
    let summary_fail = GuardrailSummary {
        total_checked: 1,
        failed: 1,
        blocked: 1,
        outcomes: vec![fail_outcome],
        ..Default::default()
    };
    assert!(
        summary_fail.should_block(),
        "Blocking Fail outcome must cause should_block() to return true"
    );
    assert_eq!(summary_fail.blocking_violations().len(), 1);

    let error_outcome = GuardrailOutcome {
        guardrail_name: "abort_on_eval_error".to_string(),
        guardrail_type: GuardrailType::PreTool,
        result: EvaluationResult::Error {
            message: "evaluation aborted with error".to_string(),
        },
        action: ViolationAction::Block,
        timestamp: std::time::Instant::now(),
        evaluation_duration_ms: 1,
    };
    let summary_error = GuardrailSummary {
        total_checked: 1,
        errors: 1,
        blocked: 1,
        outcomes: vec![error_outcome],
        ..Default::default()
    };
    assert!(
        summary_error.should_block(),
        "Blocking Error outcome must fail closed and cause should_block() to return true"
    );
    assert_eq!(summary_error.blocking_violations().len(), 1);
}

#[test]
fn test_evaluate_inline_compound_and_method_transforms() {
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new()
        .with_agent_output("agent1", "Found [CRITICAL] issue in module")
        .with_state("count", 3)
        .with_state("items", serde_json::json!(["a", "b"]));

    // .len() > 0 on string and array
    assert!(engine
        .evaluate_inline_expression("agent_output.len() > 0", &ctx)
        .is_pass());
    assert!(engine
        .evaluate_inline_expression("state.items.len() > 0", &ctx)
        .is_pass());

    // .to_lowercase().contains(...)
    assert!(engine
        .evaluate_inline_expression("agent_output.to_lowercase().contains('critical')", &ctx)
        .is_pass());
    assert!(engine
        .evaluate_inline_expression("agent_output.to_lowercase().contains('nonexistent')", &ctx)
        .is_fail());

    // Compound && and || expressions
    assert!(engine
        .evaluate_inline_expression(
            "agent_output.len() > 0 && agent_output.to_lowercase().contains('critical')",
            &ctx
        )
        .is_pass());
    assert!(engine
        .evaluate_inline_expression("state.count > 10 || state.items.len() > 0", &ctx)
        .is_pass());
    assert!(engine
        .evaluate_inline_expression(
            "state.count > 10 && agent_output.to_lowercase().contains('critical')",
            &ctx
        )
        .is_fail());
    assert!(engine
        .evaluate_inline_expression("state.count > 10 || state.count < 0", &ctx)
        .is_fail());
}

#[test]
fn test_evaluate_rust_let_bindings_map_or_and_if_else() {
    let engine = GuardrailEngine::new();

    // Test let bindings + .map_or + .unwrap_or from code_review.swl
    let code = r#"
        // Fail if critical issues found without override
        let has_critical = args.agent_outputs.security_review
          .map_or(false, |r| r.contains("[CRITICAL]"));
        let allow_critical = args.state.allow_critical.unwrap_or(false);
        !has_critical || allow_critical
    "#;

    let ctx_clean =
        GuardrailContext::new().with_agent_output("security_review", "All checks passed");
    assert!(engine.evaluate_rust_code(code, &ctx_clean).is_pass());

    let ctx_critical =
        GuardrailContext::new().with_agent_output("security_review", "[CRITICAL] RCE found");
    assert!(engine.evaluate_rust_code(code, &ctx_critical).is_fail());

    let ctx_override = GuardrailContext::new()
        .with_agent_output("security_review", "[CRITICAL] RCE found")
        .with_state("allow_critical", true);
    assert!(engine.evaluate_rust_code(code, &ctx_override).is_pass());

    // Test if / else from bug_investigation.swl
    let if_else_code = r#"
        if args.state.severity == "critical" {
          args.state.reproduction_Confirmed == true
        } else {
          true
        }
    "#;
    let ctx_non_crit = GuardrailContext::new().with_state("severity", "low");
    assert!(engine
        .evaluate_rust_code(if_else_code, &ctx_non_crit)
        .is_pass());

    let ctx_crit_unconfirmed = GuardrailContext::new().with_state("severity", "critical");
    assert!(engine
        .evaluate_rust_code(if_else_code, &ctx_crit_unconfirmed)
        .is_fail());

    let ctx_crit_confirmed = GuardrailContext::new()
        .with_state("severity", "critical")
        .with_state("reproduction_Confirmed", true);
    assert!(engine
        .evaluate_rust_code(if_else_code, &ctx_crit_confirmed)
        .is_pass());
}

#[test]
fn test_shipped_swl_workflows_guardrails_and_tools() {
    use crate::swl::lowering::lower_document;
    use crate::swl::parser::{parse_document, validate_document, GuardCondition};
    use crate::tools::ToolRegistry;

    let engine = GuardrailEngine::new();
    let registry = ToolRegistry::new();
    let files = [
        "workflows/code_review.swl",
        "workflows/bug_investigation.swl",
        "workflows/documentation_update.swl",
        "workflows/security_audit.swl",
        "workflows/product_sprint.swl",
    ];

    let empty_ctx = GuardrailContext::new();
    let populated_ctx = GuardrailContext::new()
        .with_agent_output(
            "security_review",
            "No issues found; complete analysis exceeding one hundred characters for coverage verification in security audit.",
        )
        .with_agent_output(
            "root_cause_analyzer",
            format!("Root Cause: {}", "detailed analysis ".repeat(15)),
        )
        .with_agent_output("fix_generator", "#[test] fn test_fix() {}")
        .with_agent_output(
            "recon_data",
            "recon data exceeding one hundred characters ".repeat(4),
        )
        .with_agent_output(
            "dependency_audit",
            "dependency audit exceeding one hundred characters ".repeat(4),
        )
        .with_agent_output(
            "code_security_review",
            "code security review exceeding one hundred characters ".repeat(4),
        )
        .with_agent_output(
            "owasp_findings",
            "owasp findings exceeding one hundred characters ".repeat(4),
        )
        .with_agent_output("secret_scan", "clean")
        .with_agent_output("architecture_guardian", "ok")
        .with_agent_output("qa_adversary", "ok")
        .with_agent_output("security_auditor", "ok")
        .with_agent_output("devops_lead", "ok")
        .with_state("severity", "medium")
        .with_state("tests_passed", true)
        .with_state("critical_issues_found", 0);

    for path in files {
        let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        let source = std::fs::read_to_string(&full_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", full_path.display()));
        let doc = parse_document(&source).unwrap_or_else(|e| panic!("Failed to parse {path}: {e}"));
        let issues = validate_document(&doc);
        assert!(
            issues.is_empty(),
            "{path} had validation issues: {issues:?}"
        );

        // Verify all agent tools exist in ToolRegistry
        for (agent_name, agent) in &doc.agents {
            for tool in &agent.tools {
                assert!(
                    registry.get(tool).is_some(),
                    "{path}: agent '{agent_name}' references unknown tool '{tool}'"
                );
            }
        }

        // Verify lowering succeeds
        let _lowered =
            lower_document(&doc).unwrap_or_else(|e| panic!("Failed to lower {path}: {e}"));

        // Verify every guardrail evaluates without EvaluationResult::Error
        for g in &doc.guardrails {
            let g_name = g.name.as_deref().unwrap_or("<unnamed>");
            let cond = match &g.condition {
                GuardCondition::Inline(expr) => Condition::Inline(expr.clone()),
                GuardCondition::Code(block) => Condition::Code {
                    language: block.language.as_str().to_string(),
                    content: block.code.clone(),
                },
            };

            let res_empty = engine.evaluate_condition(&cond, &empty_ctx);
            assert!(
                !res_empty.is_error(),
                "{path} guardrail '{g_name}' returned Error on empty context: {:?}",
                res_empty
            );

            let res_pop = engine.evaluate_condition(&cond, &populated_ctx);
            assert!(
                !res_pop.is_error(),
                "{path} guardrail '{g_name}' returned Error on populated context: {:?}",
                res_pop
            );
        }
    }
}
