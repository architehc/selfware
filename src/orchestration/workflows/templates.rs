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
