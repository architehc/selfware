use super::*;
use crate::cognitive::self_improvement::Outcome;

// =========================================================================
// infer_task_type — exhaustive branch coverage + edge cases
// =========================================================================

#[test]
fn test_infer_task_type_code_review() {
    assert_eq!(
        Agent::infer_task_type("Please review this PR"),
        "code_review"
    );
    assert_eq!(Agent::infer_task_type("Code review needed"), "code_review");
    assert_eq!(Agent::infer_task_type("REVIEW the changes"), "code_review");
}

#[test]
fn test_infer_task_type_testing() {
    assert_eq!(Agent::infer_task_type("Write tests for module"), "testing");
    assert_eq!(Agent::infer_task_type("Add unit test coverage"), "testing");
    assert_eq!(Agent::infer_task_type("Run TEST suite"), "testing");
}

#[test]
fn test_infer_task_type_refactor() {
    assert_eq!(Agent::infer_task_type("Refactor the parser"), "refactor");
    assert_eq!(Agent::infer_task_type("REFACTORING needed"), "refactor");
}

#[test]
fn test_infer_task_type_bug_fix() {
    assert_eq!(Agent::infer_task_type("Fix this bug"), "bug_fix");
    assert_eq!(Agent::infer_task_type("There is a bug in login"), "bug_fix");
    assert_eq!(Agent::infer_task_type("FIX compilation error"), "bug_fix");
}

#[test]
fn test_infer_task_type_documentation() {
    assert_eq!(Agent::infer_task_type("Document the API"), "documentation");
    assert_eq!(Agent::infer_task_type("Update README"), "documentation");
    assert_eq!(
        Agent::infer_task_type("Write documentation for new feature"),
        "documentation"
    );
    assert_eq!(
        Agent::infer_task_type("Update the readme file"),
        "documentation"
    );
}

#[test]
fn test_infer_task_type_general_fallback() {
    assert_eq!(Agent::infer_task_type("Build the new feature"), "general");
    assert_eq!(Agent::infer_task_type("Deploy to production"), "general");
    assert_eq!(Agent::infer_task_type(""), "general");
}

#[test]
fn test_infer_task_type_priority_order() {
    // "review" takes priority over "test" when both are present
    assert_eq!(
        Agent::infer_task_type("Review the test changes"),
        "code_review"
    );
    // "test" takes priority over "refactor"
    assert_eq!(Agent::infer_task_type("Test after refactor"), "testing");
    // "refactor" takes priority over "fix"
    assert_eq!(
        Agent::infer_task_type("Refactor to fix the issue"),
        "refactor"
    );
}

// =========================================================================
// classify_error_type — exhaustive branch coverage + edge cases
// =========================================================================

#[test]
fn test_classify_error_timeout() {
    assert_eq!(Agent::classify_error_type("request timed out"), "timeout");
    assert_eq!(
        Agent::classify_error_type("Connection timeout after 30s"),
        "timeout"
    );
    assert_eq!(Agent::classify_error_type("Operation TIMED OUT"), "timeout");
}

#[test]
fn test_classify_error_permission() {
    assert_eq!(
        Agent::classify_error_type("permission denied"),
        "permission"
    );
    assert_eq!(
        Agent::classify_error_type("Access denied for path /root"),
        "permission"
    );
    assert_eq!(Agent::classify_error_type("PERMISSION error"), "permission");
}

#[test]
fn test_classify_error_safety() {
    assert_eq!(Agent::classify_error_type("Safety check failed"), "safety");
    assert_eq!(
        Agent::classify_error_type("Operation blocked by policy"),
        "safety"
    );
    assert_eq!(Agent::classify_error_type("BLOCKED by firewall"), "safety");
}

#[test]
fn test_classify_error_parsing() {
    assert_eq!(
        Agent::classify_error_type("Invalid JSON in response"),
        "parsing"
    );
    assert_eq!(
        Agent::classify_error_type("Failed to parse config"),
        "parsing"
    );
    assert_eq!(Agent::classify_error_type("JSON decode error"), "parsing");
    assert_eq!(
        Agent::classify_error_type("invalid syntax at line 5"),
        "parsing"
    );
}

#[test]
fn test_classify_error_network() {
    assert_eq!(Agent::classify_error_type("Network unreachable"), "network");
    assert_eq!(Agent::classify_error_type("Connection refused"), "network");
    assert_eq!(Agent::classify_error_type("NETWORK error"), "network");
}

#[test]
fn test_classify_error_execution_fallback() {
    assert_eq!(
        Agent::classify_error_type("unknown error occurred"),
        "execution"
    );
    assert_eq!(Agent::classify_error_type("segfault"), "execution");
    assert_eq!(Agent::classify_error_type(""), "execution");
}

#[test]
fn test_classify_error_priority_order() {
    // "timeout" takes priority over "network" when both keywords present
    assert_eq!(
        Agent::classify_error_type("network connection timeout"),
        "timeout"
    );
    // "permission" takes priority over "safety"
    assert_eq!(
        Agent::classify_error_type("permission denied: safety policy"),
        "permission"
    );
}

// =========================================================================
// outcome_quality — all variants
// =========================================================================

#[test]
fn test_outcome_quality_all_variants() {
    assert_eq!(Agent::outcome_quality(Outcome::Success), 1.0);
    assert_eq!(Agent::outcome_quality(Outcome::Partial), 0.65);
    assert_eq!(Agent::outcome_quality(Outcome::Failure), 0.0);
    assert_eq!(Agent::outcome_quality(Outcome::Abandoned), 0.2);
}

#[test]
fn test_outcome_quality_ordering() {
    assert!(Agent::outcome_quality(Outcome::Success) > Agent::outcome_quality(Outcome::Partial));
    assert!(Agent::outcome_quality(Outcome::Partial) > Agent::outcome_quality(Outcome::Abandoned));
    assert!(Agent::outcome_quality(Outcome::Abandoned) > Agent::outcome_quality(Outcome::Failure));
}

// =========================================================================
// Outcome enum methods
// =========================================================================

#[test]
fn test_outcome_is_positive() {
    assert!(Outcome::Success.is_positive());
    assert!(Outcome::Partial.is_positive());
    assert!(!Outcome::Failure.is_positive());
    assert!(!Outcome::Abandoned.is_positive());
}

#[test]
fn test_outcome_score() {
    assert_eq!(Outcome::Success.score(), 1.0);
    assert_eq!(Outcome::Partial.score(), 0.5);
    assert_eq!(Outcome::Failure.score(), 0.0);
    assert_eq!(Outcome::Abandoned.score(), 0.0);
}

// =========================================================================
// build_learning_hint edge cases (via mock Agent)
// =========================================================================

#[tokio::test]
async fn test_build_learning_hint_empty_prompt_returns_none() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("done")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock".to_string(),
        agent: crate::config::AgentConfig {
            max_iterations: 5,
            step_timeout_secs: 5,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let agent = Agent::new(config).await.unwrap();

    // Empty prompt should return None
    assert!(agent.build_learning_hint("").is_none());
    assert!(agent.build_learning_hint("   ").is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_build_learning_hint_returns_string_or_none() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("done")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock".to_string(),
        agent: crate::config::AgentConfig {
            max_iterations: 5,
            step_timeout_secs: 5,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let agent = Agent::new(config).await.unwrap();

    let result = agent.build_learning_hint("Write a new parser");
    // With no recorded data, result is None; with persisted data it contains guidance
    match result {
        None => {} // Fresh engine — no hints
        Some(ref hint) => {
            assert!(
                hint.contains("Self-improvement guidance"),
                "hint should contain expected prefix: {}",
                hint
            );
        }
    }

    server.stop().await;
}

// =========================================================================
// learning_context
// =========================================================================

#[tokio::test]
async fn test_learning_context_defaults_to_general() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("done")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock".to_string(),
        agent: crate::config::AgentConfig {
            max_iterations: 5,
            step_timeout_secs: 5,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let agent = Agent::new(config).await.unwrap();

    // Default empty context returns "general"
    assert_eq!(agent.learning_context(), "general");

    server.stop().await;
}
