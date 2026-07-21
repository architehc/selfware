use super::*;

#[tokio::test]
async fn test_guarded_runtime_creation() {
    let runtime = GuardedSwlRuntime::new_dry_run();
    // Should create without panicking
    let telemetry = runtime.get_guardrail_telemetry().await;
    assert!(telemetry.is_empty());
}

#[test]
fn test_runtime_builder() {
    let runtime = GuardedRuntimeBuilder::new()
        .with_dry_run()
        .with_verbose_guardrails()
        .build();
    // Should create without panicking
}

#[test]
fn test_clone_preserves_real_base_runtime() {
    // Regression: clone() used to swap in `SwlRuntime::new_dry_run()`, so
    // guarded parallel and map-reduce workflows returned `[DRY-RUN]`
    // placeholder strings from every spawned agent while reporting
    // `ExecutionStatus::Completed`.
    let client = Arc::new(ApiClient::new(&crate::config::Config::default()).unwrap());
    let runtime = GuardedSwlRuntime::new(client).with_max_tool_iterations(7);

    let cloned = runtime.clone();

    assert!(
        !cloned.base.dry_run,
        "cloning a live runtime must not produce a dry-run base"
    );
    assert_eq!(cloned.base.max_tool_iterations, 7);
    assert!(Arc::ptr_eq(&cloned.enforcer, &runtime.enforcer));
    assert!(Arc::ptr_eq(
        &cloned.current_workflow,
        &runtime.current_workflow
    ));
}

#[test]
fn test_clone_preserves_dry_run_base() {
    let runtime = GuardedSwlRuntime::new_dry_run();
    assert!(
        runtime.clone().base.dry_run,
        "cloning a dry-run runtime must stay dry-run"
    );
}

#[test]
fn test_condition_result_is_true_exact_matching() {
    assert!(condition_result_is_true("true"));
    assert!(condition_result_is_true("TRUE"));
    assert!(condition_result_is_true("  true \n"));
    assert!(condition_result_is_true("yes"));
    assert!(condition_result_is_true("1"));

    // Substring matches must NOT count (the old code used
    // `contains("true")`, so these all incorrectly ran the branch).
    assert!(!condition_result_is_true("untrue"));
    assert!(!condition_result_is_true("true_value"));
    assert!(!condition_result_is_true("The answer is true."));
    assert!(!condition_result_is_true("false"));
    assert!(!condition_result_is_true(""));
}

fn test_agent() -> AgentDefinition {
    AgentDefinition {
        model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
        role: None,
        instruction: None,
        tools: vec![],
        output_key: None,
        sub_agents: vec![],
    }
}

fn test_doc() -> SwlDocument {
    let mut agents = std::collections::BTreeMap::new();
    agents.insert("mapper".to_string(), test_agent());
    agents.insert("reducer".to_string(), test_agent());

    SwlDocument {
        version: "1.0".to_string(),
        name: "test".to_string(),
        description: None,
        metadata: None,
        agents,
        workflows: std::collections::BTreeMap::new(),
        guardrails: vec![],
        telemetry: None,
        dashboard: None,
        state: None,
    }
}

#[test]
fn test_select_reduce_agent_honors_declared_agent() {
    // Regression: the guarded map-reduce ignored the declared reduce
    // agent and always used the document's last agent.
    let doc = test_doc();
    let workflow = WorkflowDefinition {
        workflow_type: WorkflowType::MapReduce,
        description: None,
        steps: vec![],
        map: None,
        reduce: Some(ReduceStage::Aggregate(
            crate::swl::parser::ast::AggregateStage {
                agent: "reducer".to_string(),
                instruction: None,
                inputs: vec![],
            },
        )),
        merge: None,
    };

    assert_eq!(
        select_reduce_agent(&workflow, &doc).as_deref(),
        Some("reducer")
    );
}

#[test]
fn test_select_reduce_agent_code_falls_back_to_last_agent() {
    let doc = test_doc();
    let workflow = WorkflowDefinition {
        workflow_type: WorkflowType::MapReduce,
        description: None,
        steps: vec![],
        map: None,
        reduce: Some(ReduceStage::Code(crate::swl::parser::ast::CodeBlock {
            language: crate::swl::parser::ast::CodeLanguage::Rust,
            code: "true".to_string(),
        })),
        merge: None,
    };

    // BTreeMap ordering: "reducer" > "mapper"
    assert_eq!(
        select_reduce_agent(&workflow, &doc).as_deref(),
        Some("reducer")
    );
}

#[test]
fn test_select_reduce_agent_none_without_reduce_stage() {
    let doc = test_doc();
    let workflow = WorkflowDefinition {
        workflow_type: WorkflowType::MapReduce,
        description: None,
        steps: vec![],
        map: None,
        reduce: None,
        merge: None,
    };

    assert_eq!(select_reduce_agent(&workflow, &doc), None);
}

#[tokio::test]
async fn test_collect_agent_outputs_success() {
    let a: tokio::task::JoinHandle<crate::errors::Result<(String, String)>> =
        tokio::spawn(async { Ok(("a".to_string(), "1".to_string())) });
    let b: tokio::task::JoinHandle<crate::errors::Result<(String, String)>> =
        tokio::spawn(async { Ok(("b".to_string(), "2".to_string())) });

    let outputs = collect_agent_outputs(vec![a, b]).await.unwrap();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs.get("a").map(String::as_str), Some("1"));
    assert_eq!(outputs.get("b").map(String::as_str), Some("2"));
}

#[tokio::test]
async fn test_collect_agent_outputs_propagates_agent_errors() {
    let failing: tokio::task::JoinHandle<crate::errors::Result<(String, String)>> =
        tokio::spawn(async { Err(SelfwareError::Internal("agent exploded".to_string())) });

    let result = collect_agent_outputs(vec![failing]).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("agent exploded"));
}

#[tokio::test]
async fn test_collect_agent_outputs_propagates_panics() {
    // Regression: a panicked agent task was logged and dropped, and the
    // workflow went on to report Completed with partial outputs.
    let ok: tokio::task::JoinHandle<crate::errors::Result<(String, String)>> =
        tokio::spawn(async { Ok(("good".to_string(), "out".to_string())) });
    let panicky: tokio::task::JoinHandle<crate::errors::Result<(String, String)>> =
        tokio::spawn(async {
            panic!("boom");
        });

    let result = collect_agent_outputs(vec![ok, panicky]).await;
    let err = result.expect_err("a panicked agent task must fail the workflow");
    assert!(
        err.to_string().contains("panicked"),
        "unexpected error: {}",
        err
    );
}
