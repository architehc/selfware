use super::*;

#[test]
fn test_workflow_templates_tdd() {
    let workflow = WorkflowTemplates::tdd();
    assert_eq!(workflow.name, "tdd");
    assert!(!workflow.steps.is_empty());
    assert!(workflow.tags.contains(&"tdd".to_string()));
}

#[test]
fn test_workflow_templates_debug() {
    let workflow = WorkflowTemplates::debug();
    assert_eq!(workflow.name, "debug");
    assert_eq!(workflow.category, "debugging");
}

#[test]
fn test_workflow_templates_review() {
    let workflow = WorkflowTemplates::review();
    assert_eq!(workflow.name, "review");
    assert!(workflow.inputs.iter().any(|i| i.name == "files"));
}

#[test]
fn test_workflow_templates_refactor() {
    let workflow = WorkflowTemplates::refactor();
    assert_eq!(workflow.name, "refactor");
    assert!(workflow.steps.iter().any(|s| s.id == "run_tests_before"));
    assert!(workflow.steps.iter().any(|s| s.id == "run_tests_after"));
}

#[test]
fn test_tdd_template_metadata() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.name, "tdd");
    assert_eq!(wf.description, "Test-Driven Development workflow");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "development");
}

#[test]
fn test_tdd_template_inputs() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.inputs.len(), 2);

    let feature_input = &wf.inputs[0];
    assert_eq!(feature_input.name, "feature");
    assert!(feature_input.required);
    assert!(feature_input.default.is_none());

    let test_file_input = &wf.inputs[1];
    assert_eq!(test_file_input.name, "test_file");
    assert!(!test_file_input.required);
    assert!(test_file_input.default.is_some());
    assert_eq!(
        test_file_input.default.as_ref().unwrap().as_string(),
        Some("tests/test_feature.rs".to_string())
    );
}

#[test]
fn test_tdd_template_outputs() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.outputs.len(), 1);
    assert_eq!(wf.outputs[0].name, "test_passed");
    assert_eq!(wf.outputs[0].from, "tests_passed");
}

#[test]
fn test_tdd_template_step_count_and_ids() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.steps.len(), 5);
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "write_test",
            "run_test_red",
            "implement",
            "run_test_green",
            "refactor"
        ]
    );
}

#[test]
fn test_tdd_template_dependency_chain() {
    let wf = WorkflowTemplates::tdd();
    // First step has no dependencies
    assert!(wf.steps[0].depends_on.is_empty());
    // Each subsequent step depends on the previous one
    assert_eq!(wf.steps[1].depends_on, vec!["write_test"]);
    assert_eq!(wf.steps[2].depends_on, vec!["run_test_red"]);
    assert_eq!(wf.steps[3].depends_on, vec!["implement"]);
    assert_eq!(wf.steps[4].depends_on, vec!["run_test_green"]);
}

#[test]
fn test_tdd_template_step_required_flags() {
    let wf = WorkflowTemplates::tdd();
    // write_test: required
    assert!(wf.steps[0].required);
    // run_test_red: not required (expected to fail in red phase)
    assert!(!wf.steps[1].required);
    // implement: required
    assert!(wf.steps[2].required);
    // run_test_green: required
    assert!(wf.steps[3].required);
    // refactor: not required
    assert!(!wf.steps[4].required);
}

#[test]
fn test_tdd_template_green_step_retry_config() {
    let wf = WorkflowTemplates::tdd();
    let green_step = &wf.steps[3];
    assert_eq!(green_step.id, "run_test_green");
    assert_eq!(green_step.retry.max_attempts, 3);
    assert_eq!(green_step.retry.delay_secs, 5);
    assert!(!green_step.retry.exponential);
}

#[test]
fn test_tdd_template_tags() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.tags.len(), 3);
    assert!(wf.tags.contains(&"tdd".to_string()));
    assert!(wf.tags.contains(&"testing".to_string()));
    assert!(wf.tags.contains(&"development".to_string()));
}

#[test]
fn test_debug_template_metadata() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.name, "debug");
    assert_eq!(wf.description, "Debugging workflow");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "debugging");
}

#[test]
fn test_debug_template_single_required_input() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.inputs.len(), 1);
    assert_eq!(wf.inputs[0].name, "issue");
    assert!(wf.inputs[0].required);
    assert!(wf.inputs[0].default.is_none());
}

#[test]
fn test_debug_template_no_outputs() {
    let wf = WorkflowTemplates::debug();
    assert!(wf.outputs.is_empty());
}

#[test]
fn test_debug_template_step_chain() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.steps.len(), 4);
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["reproduce", "analyze", "fix", "verify"]);

    assert!(wf.steps[0].depends_on.is_empty());
    assert_eq!(wf.steps[1].depends_on, vec!["reproduce"]);
    assert_eq!(wf.steps[2].depends_on, vec!["analyze"]);
    assert_eq!(wf.steps[3].depends_on, vec!["fix"]);
}

#[test]
fn test_debug_template_all_steps_required() {
    let wf = WorkflowTemplates::debug();
    for step in &wf.steps {
        assert!(step.required, "Debug step '{}' should be required", step.id);
    }
}

#[test]
fn test_review_template_parallel_initial_steps() {
    let wf = WorkflowTemplates::review();
    // check_style, review_logic, and check_security have no dependencies (can run in parallel)
    assert!(wf.steps[0].depends_on.is_empty()); // check_style
    assert!(wf.steps[1].depends_on.is_empty()); // review_logic
    assert!(wf.steps[2].depends_on.is_empty()); // check_security

    // summarize depends on all three
    let summarize = &wf.steps[3];
    assert_eq!(summarize.id, "summarize");
    assert_eq!(summarize.depends_on.len(), 3);
    assert!(summarize.depends_on.contains(&"check_style".to_string()));
    assert!(summarize.depends_on.contains(&"review_logic".to_string()));
    assert!(summarize.depends_on.contains(&"check_security".to_string()));
}

#[test]
fn test_review_template_metadata() {
    let wf = WorkflowTemplates::review();
    assert_eq!(wf.name, "review");
    assert_eq!(wf.category, "review");
    assert_eq!(wf.tags, vec!["review", "code-quality"]);
}

#[test]
fn test_refactor_template_bookend_test_steps() {
    let wf = WorkflowTemplates::refactor();
    // First and last steps are shell test commands
    let first = &wf.steps[0];
    assert_eq!(first.id, "run_tests_before");
    assert!(matches!(first.step_type, StepType::Shell { .. }));

    let last = &wf.steps[3];
    assert_eq!(last.id, "run_tests_after");
    assert!(matches!(last.step_type, StepType::Shell { .. }));
}

#[test]
fn test_refactor_template_two_required_inputs() {
    let wf = WorkflowTemplates::refactor();
    assert_eq!(wf.inputs.len(), 2);
    assert_eq!(wf.inputs[0].name, "target");
    assert!(wf.inputs[0].required);
    assert_eq!(wf.inputs[1].name, "goal");
    assert!(wf.inputs[1].required);
}

#[test]
fn test_refactor_template_after_step_retry() {
    let wf = WorkflowTemplates::refactor();
    let after_step = &wf.steps[3];
    assert_eq!(after_step.id, "run_tests_after");
    assert_eq!(after_step.retry.max_attempts, 2);
    assert_eq!(after_step.retry.delay_secs, 5);
    assert!(!after_step.retry.exponential);
}

#[test]
fn test_all_templates_have_version_1_0_0() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert_eq!(
            wf.version, "1.0.0",
            "Template '{}' should have version 1.0.0",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_author_is_selfware() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert_eq!(
            wf.author, "Selfware",
            "Template '{}' should have author Selfware",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_have_unique_step_ids() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        let mut ids = std::collections::HashSet::new();
        for step in &wf.steps {
            assert!(
                ids.insert(step.id.clone()),
                "Duplicate step id '{}' in template '{}'",
                step.id,
                wf.name
            );
        }
    }
}

#[test]
fn test_all_templates_dependencies_reference_valid_steps() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        let step_ids: std::collections::HashSet<String> =
            wf.steps.iter().map(|s| s.id.clone()).collect();
        for step in &wf.steps {
            for dep in &step.depends_on {
                assert!(
                    step_ids.contains(dep),
                    "Step '{}' in template '{}' depends on unknown step '{}'",
                    step.id,
                    wf.name,
                    dep
                );
            }
        }
    }
}

#[test]
fn test_all_templates_have_at_least_one_step() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(
            !wf.steps.is_empty(),
            "Template '{}' should have at least one step",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_have_non_empty_tags() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(
            !wf.tags.is_empty(),
            "Template '{}' should have at least one tag",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_steps_have_timeouts() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        for step in &wf.steps {
            assert!(
                step.timeout_secs.is_some(),
                "Step '{}' in template '{}' should have a timeout",
                step.id,
                wf.name
            );
        }
    }
}

#[test]
fn test_tdd_template_variable_substitution_in_prompts() {
    let wf = WorkflowTemplates::tdd();
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("feature", "user login");
    ctx.set_var("test_file", "tests/login_test.rs");

    // write_test step uses ${feature} placeholder
    if let StepType::Llm {
        ref prompt,
        ref context,
    } = wf.steps[0].step_type
    {
        let substituted = ctx.substitute(prompt);
        assert_eq!(substituted, "Write a failing test for: user login");
        let ctx_sub = ctx.substitute(&context[0]);
        assert_eq!(ctx_sub, "tests/login_test.rs");
    } else {
        panic!("Expected LLM step type for write_test");
    }
}
