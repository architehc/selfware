use super::*;
use std::collections::HashSet;

// --- TDD template tests ---

#[test]
fn test_tdd_template_step_count() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.steps.len(), 5);
}

#[test]
fn test_tdd_template_step_ids() {
    let wf = WorkflowTemplates::tdd();
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
fn test_tdd_template_dependency_ordering() {
    let wf = WorkflowTemplates::tdd();
    // First step has no deps
    assert!(wf.steps[0].depends_on.is_empty());
    // Each subsequent step depends on the prior
    assert_eq!(wf.steps[1].depends_on, vec!["write_test"]);
    assert_eq!(wf.steps[2].depends_on, vec!["run_test_red"]);
    assert_eq!(wf.steps[3].depends_on, vec!["implement"]);
    assert_eq!(wf.steps[4].depends_on, vec!["run_test_green"]);
}

#[test]
fn test_tdd_template_metadata() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.name, "tdd");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "development");
}

// --- Debug template tests ---

#[test]
fn test_debug_template_step_count() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.steps.len(), 4);
}

#[test]
fn test_debug_template_step_ids() {
    let wf = WorkflowTemplates::debug();
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["reproduce", "analyze", "fix", "verify"]);
}

#[test]
fn test_debug_template_dependency_ordering() {
    let wf = WorkflowTemplates::debug();
    assert!(wf.steps[0].depends_on.is_empty());
    assert_eq!(wf.steps[1].depends_on, vec!["reproduce"]);
    assert_eq!(wf.steps[2].depends_on, vec!["analyze"]);
    assert_eq!(wf.steps[3].depends_on, vec!["fix"]);
}

#[test]
fn test_debug_template_metadata() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.name, "debug");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "debugging");
}

// --- Review template tests ---

#[test]
fn test_review_template_step_count() {
    let wf = WorkflowTemplates::review();
    assert_eq!(wf.steps.len(), 4);
}

#[test]
fn test_review_template_step_ids() {
    let wf = WorkflowTemplates::review();
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["check_style", "review_logic", "check_security", "summarize"]
    );
}

#[test]
fn test_review_template_dependency_ordering() {
    let wf = WorkflowTemplates::review();
    // First 3 steps have no deps (run in parallel)
    assert!(wf.steps[0].depends_on.is_empty());
    assert!(wf.steps[1].depends_on.is_empty());
    assert!(wf.steps[2].depends_on.is_empty());
    // Summarize depends on all three
    let summarize = &wf.steps[3];
    assert_eq!(summarize.depends_on.len(), 3);
    assert!(summarize.depends_on.contains(&"check_style".to_string()));
    assert!(summarize.depends_on.contains(&"review_logic".to_string()));
    assert!(summarize.depends_on.contains(&"check_security".to_string()));
}

#[test]
fn test_review_template_metadata() {
    let wf = WorkflowTemplates::review();
    assert_eq!(wf.name, "review");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "review");
}

// --- Refactor template tests ---

#[test]
fn test_refactor_template_step_count() {
    let wf = WorkflowTemplates::refactor();
    assert_eq!(wf.steps.len(), 4);
}

#[test]
fn test_refactor_template_step_ids() {
    let wf = WorkflowTemplates::refactor();
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["run_tests_before", "analyze", "refactor", "run_tests_after"]
    );
}

#[test]
fn test_refactor_template_dependency_ordering() {
    let wf = WorkflowTemplates::refactor();
    assert!(wf.steps[0].depends_on.is_empty());
    assert_eq!(wf.steps[1].depends_on, vec!["run_tests_before"]);
    assert_eq!(wf.steps[2].depends_on, vec!["analyze"]);
    assert_eq!(wf.steps[3].depends_on, vec!["refactor"]);
}

#[test]
fn test_refactor_template_metadata() {
    let wf = WorkflowTemplates::refactor();
    assert_eq!(wf.name, "refactor");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "development");
}

// --- Cross-template validation tests ---

#[test]
fn test_all_templates_have_non_empty_steps() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(!wf.steps.is_empty(), "Template '{}' has no steps", wf.name);
    }
}

#[test]
fn test_all_templates_have_version_and_author() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(
            !wf.version.is_empty(),
            "Template '{}' has empty version",
            wf.name
        );
        assert!(
            !wf.author.is_empty(),
            "Template '{}' has empty author",
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
        let mut ids = HashSet::new();
        for step in &wf.steps {
            assert!(
                ids.insert(&step.id),
                "Template '{}' has duplicate step id '{}'",
                wf.name,
                step.id
            );
        }
    }
}

#[test]
fn test_all_templates_dependencies_reference_valid_step_ids() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        let valid_ids: HashSet<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
        for step in &wf.steps {
            for dep in &step.depends_on {
                assert!(
                    valid_ids.contains(dep.as_str()),
                    "Template '{}' step '{}' depends on non-existent step '{}'",
                    wf.name,
                    step.id,
                    dep
                );
            }
        }
    }
}

#[test]
fn test_all_templates_have_non_empty_descriptions() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(
            !wf.description.is_empty(),
            "Template '{}' has empty description",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_have_tags() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(!wf.tags.is_empty(), "Template '{}' has no tags", wf.name);
    }
}
