use super::*;

impl WorkflowTemplates {
    /// TDD workflow template
    pub fn tdd() -> Workflow {
        Workflow {
            name: "tdd".to_string(),
            description: "Test-Driven Development workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "development".to_string(),
            inputs: vec![
                WorkflowInput {
                    name: "feature".to_string(),
                    description: "Feature to implement".to_string(),
                    required: true,
                    default: None,
                    param_type: "string".to_string(),
                },
                WorkflowInput {
                    name: "test_file".to_string(),
                    description: "Test file path".to_string(),
                    required: false,
                    default: Some(VarValue::String("tests/test_feature.rs".to_string())),
                    param_type: "string".to_string(),
                },
            ],
            outputs: vec![WorkflowOutput {
                name: "test_passed".to_string(),
                description: "Whether tests passed".to_string(),
                from: "tests_passed".to_string(),
            }],
            steps: vec![
                WorkflowStep {
                    id: "write_test".to_string(),
                    name: "Write failing test".to_string(),
                    description: "Write a test that fails".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Write a failing test for: ${feature}".to_string(),
                        context: vec!["${test_file}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "run_test_red".to_string(),
                    name: "Verify test fails".to_string(),
                    description: "Run test to confirm it fails".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: false, // Expected to fail
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["write_test".to_string()],
                },
                WorkflowStep {
                    id: "implement".to_string(),
                    name: "Implement feature".to_string(),
                    description: "Write code to make test pass".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Implement the feature to make the test pass: ${feature}"
                            .to_string(),
                        context: vec!["${test_file}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["run_test_red".to_string()],
                },
                WorkflowStep {
                    id: "run_test_green".to_string(),
                    name: "Verify test passes".to_string(),
                    description: "Run test to confirm it passes".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig {
                        max_attempts: 3,
                        delay_secs: 5,
                        exponential: false,
                    },
                    timeout_secs: Some(120),
                    depends_on: vec!["implement".to_string()],
                },
                WorkflowStep {
                    id: "refactor".to_string(),
                    name: "Refactor if needed".to_string(),
                    description: "Clean up the implementation".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Review and refactor the implementation if needed".to_string(),
                        context: vec![],
                    },
                    required: false,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec!["run_test_green".to_string()],
                },
            ],
            tags: vec![
                "tdd".to_string(),
                "testing".to_string(),
                "development".to_string(),
            ],
        }
    }

    /// Debug workflow template
    pub fn debug() -> Workflow {
        Workflow {
            name: "debug".to_string(),
            description: "Debugging workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "debugging".to_string(),
            inputs: vec![WorkflowInput {
                name: "issue".to_string(),
                description: "Issue or error to debug".to_string(),
                required: true,
                default: None,
                param_type: "string".to_string(),
            }],
            outputs: vec![],
            steps: vec![
                WorkflowStep {
                    id: "reproduce".to_string(),
                    name: "Reproduce issue".to_string(),
                    description: "Attempt to reproduce the issue".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Analyze and try to reproduce: ${issue}".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "analyze".to_string(),
                    name: "Analyze root cause".to_string(),
                    description: "Find the root cause".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Find the root cause of the issue".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["reproduce".to_string()],
                },
                WorkflowStep {
                    id: "fix".to_string(),
                    name: "Implement fix".to_string(),
                    description: "Fix the issue".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Implement a fix for the root cause".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["analyze".to_string()],
                },
                WorkflowStep {
                    id: "verify".to_string(),
                    name: "Verify fix".to_string(),
                    description: "Verify the fix works".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["fix".to_string()],
                },
            ],
            tags: vec!["debug".to_string(), "bugfix".to_string()],
        }
    }

    /// Code review workflow template
    pub fn review() -> Workflow {
        Workflow {
            name: "review".to_string(),
            description: "Code review workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "review".to_string(),
            inputs: vec![WorkflowInput {
                name: "files".to_string(),
                description: "Files to review".to_string(),
                required: true,
                default: None,
                param_type: "string".to_string(),
            }],
            outputs: vec![],
            steps: vec![
                WorkflowStep {
                    id: "check_style".to_string(),
                    name: "Check code style".to_string(),
                    description: "Run linter and formatter checks".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo clippy".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "review_logic".to_string(),
                    name: "Review logic".to_string(),
                    description: "Review code logic and design".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Review the following files for logic issues: ${files}".to_string(),
                        context: vec!["${files}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(180),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "check_security".to_string(),
                    name: "Security review".to_string(),
                    description: "Check for security issues".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Review for security vulnerabilities: ${files}".to_string(),
                        context: vec!["${files}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "summarize".to_string(),
                    name: "Summarize findings".to_string(),
                    description: "Create review summary".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Summarize all review findings".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec![
                        "check_style".to_string(),
                        "review_logic".to_string(),
                        "check_security".to_string(),
                    ],
                },
            ],
            tags: vec!["review".to_string(), "code-quality".to_string()],
        }
    }

    /// Refactor workflow template
    pub fn refactor() -> Workflow {
        Workflow {
            name: "refactor".to_string(),
            description: "Refactoring workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "development".to_string(),
            inputs: vec![
                WorkflowInput {
                    name: "target".to_string(),
                    description: "Code to refactor".to_string(),
                    required: true,
                    default: None,
                    param_type: "string".to_string(),
                },
                WorkflowInput {
                    name: "goal".to_string(),
                    description: "Refactoring goal".to_string(),
                    required: true,
                    default: None,
                    param_type: "string".to_string(),
                },
            ],
            outputs: vec![],
            steps: vec![
                WorkflowStep {
                    id: "run_tests_before".to_string(),
                    name: "Run tests before".to_string(),
                    description: "Ensure tests pass before refactoring".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "analyze".to_string(),
                    name: "Analyze code".to_string(),
                    description: "Analyze code structure".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Analyze ${target} for refactoring to achieve: ${goal}".to_string(),
                        context: vec!["${target}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["run_tests_before".to_string()],
                },
                WorkflowStep {
                    id: "refactor".to_string(),
                    name: "Apply refactoring".to_string(),
                    description: "Apply the refactoring changes".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Apply the planned refactoring changes".to_string(),
                        context: vec!["${target}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(180),
                    depends_on: vec!["analyze".to_string()],
                },
                WorkflowStep {
                    id: "run_tests_after".to_string(),
                    name: "Run tests after".to_string(),
                    description: "Ensure tests still pass".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig {
                        max_attempts: 2,
                        delay_secs: 5,
                        exponential: false,
                    },
                    timeout_secs: Some(120),
                    depends_on: vec!["refactor".to_string()],
                },
            ],
            tags: vec!["refactor".to_string(), "development".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
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
}
