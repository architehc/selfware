use super::*;

// =========================================================================
// is_confirmation_error tests
// =========================================================================

#[test]
fn test_is_confirmation_error_with_selfware_agent_wrapper() {
    // SelfwareError::Agent(AgentError::ConfirmationRequired) wrapped in anyhow
    let err = SelfwareError::Agent(AgentError::ConfirmationRequired {
        tool_name: "shell_exec".to_string(),
    });
    let anyhow_err: anyhow::Error = err.into();
    assert!(
        is_confirmation_error(&anyhow_err),
        "SelfwareError::Agent(ConfirmationRequired) should be detected"
    );
}

#[test]
fn test_is_confirmation_error_with_direct_agent_error() {
    // AgentError::ConfirmationRequired put directly into anyhow (no SelfwareError wrapper)
    let err: anyhow::Error = AgentError::ConfirmationRequired {
        tool_name: "file_write".to_string(),
    }
    .into();
    assert!(
        is_confirmation_error(&err),
        "Direct AgentError::ConfirmationRequired should be detected"
    );
}

#[test]
fn test_is_confirmation_error_plain_anyhow() {
    let err = anyhow::anyhow!("something went wrong");
    assert!(
        !is_confirmation_error(&err),
        "Plain anyhow error should not be a confirmation error"
    );
}

#[test]
fn test_is_confirmation_error_api_error() {
    let err: anyhow::Error = SelfwareError::Api(ApiError::Timeout).into();
    assert!(
        !is_confirmation_error(&err),
        "ApiError::Timeout should not be a confirmation error"
    );
}

#[test]
fn test_is_confirmation_error_tool_error() {
    let err: anyhow::Error = SelfwareError::Tool(ToolError::NotFound {
        name: "missing_tool".to_string(),
    })
    .into();
    assert!(
        !is_confirmation_error(&err),
        "ToolError should not be a confirmation error"
    );
}

#[test]
fn test_is_confirmation_error_safety_error() {
    let err: anyhow::Error = SelfwareError::Safety(SafetyError::BlockedPath {
        path: "/etc/passwd".to_string(),
    })
    .into();
    assert!(
        !is_confirmation_error(&err),
        "SafetyError should not be a confirmation error"
    );
}

#[test]
fn test_is_confirmation_error_safety_blocked_path() {
    // SafetyError variants should not be detected as confirmation errors
    let err: anyhow::Error = SelfwareError::Safety(SafetyError::BlockedPath {
        path: "/etc/shadow".to_string(),
    })
    .into();
    assert!(
        !is_confirmation_error(&err),
        "SafetyError::BlockedPath is not the agent-level confirmation error"
    );
}

#[test]
fn test_is_confirmation_error_other_agent_errors() {
    let cases: Vec<AgentError> = vec![
        AgentError::IterationLimit { limit: 10 },
        AgentError::StepTimeout { seconds: 30 },
        AgentError::Cancelled,
        AgentError::MissingSystemPrompt,
        AgentError::Panic("oops".to_string()),
        AgentError::InvalidStateTransition {
            from: "A".to_string(),
            to: "B".to_string(),
        },
    ];
    for agent_err in cases {
        let display = format!("{}", agent_err);
        let err: anyhow::Error = agent_err.into();
        assert!(
            !is_confirmation_error(&err),
            "AgentError '{}' should not be a confirmation error",
            display
        );
    }
}

// =========================================================================
// get_exit_code tests
// =========================================================================

#[test]
fn test_exit_code_confirmation_required_via_selfware_wrapper() {
    let err: anyhow::Error = SelfwareError::Agent(AgentError::ConfirmationRequired {
        tool_name: "shell_exec".to_string(),
    })
    .into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_CONFIRMATION_REQUIRED,
        "ConfirmationRequired should yield exit code 6"
    );
}

#[test]
fn test_exit_code_confirmation_required_direct() {
    let err: anyhow::Error = AgentError::ConfirmationRequired {
        tool_name: "git_push".to_string(),
    }
    .into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_CONFIRMATION_REQUIRED,
        "Direct AgentError::ConfirmationRequired should yield exit code 6"
    );
}

#[test]
fn test_exit_code_config_error() {
    let err: anyhow::Error = SelfwareError::Config("missing API key".to_string()).into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_CONFIG_ERROR,
        "Config error should yield exit code 2"
    );
}

#[test]
fn test_exit_code_api_error_wrapped() {
    let err: anyhow::Error =
        SelfwareError::Api(ApiError::Authentication("bad key".to_string())).into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_API_ERROR,
        "Api error should yield exit code 4"
    );
}

#[test]
fn test_exit_code_api_error_direct() {
    // ApiError placed directly into anyhow (not wrapped in SelfwareError)
    let err: anyhow::Error = ApiError::Timeout.into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_API_ERROR,
        "Direct ApiError should yield exit code 4"
    );
}

#[test]
fn test_exit_code_safety_error_wrapped() {
    let err: anyhow::Error = SelfwareError::Safety(SafetyError::BlockedCommand {
        command: "rm -rf /".to_string(),
        reason: "dangerous".to_string(),
    })
    .into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_SAFETY_ERROR,
        "Safety error should yield exit code 5"
    );
}

#[test]
fn test_exit_code_safety_error_direct() {
    let err: anyhow::Error = SafetyError::SecretDetected {
        finding: "AWS key".to_string(),
    }
    .into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_SAFETY_ERROR,
        "Direct SafetyError should yield exit code 5"
    );
}

#[test]
fn test_exit_code_agent_error_non_confirmation() {
    // Non-confirmation AgentError should yield generic EXIT_ERROR
    let err: anyhow::Error = SelfwareError::Agent(AgentError::IterationLimit { limit: 50 }).into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_ERROR,
        "Non-confirmation agent error should yield exit code 1"
    );
}

#[test]
fn test_exit_code_tool_error() {
    let err: anyhow::Error = SelfwareError::Tool(ToolError::Execution {
        name: "shell_exec".to_string(),
        message: "command not found".to_string(),
    })
    .into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_ERROR,
        "Tool error should yield exit code 1"
    );
}

#[test]
fn test_exit_code_session_error() {
    let err: anyhow::Error =
        SelfwareError::Session(SessionError::CheckpointSave("disk full".to_string())).into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_ERROR,
        "Session error should yield exit code 1"
    );
}

#[test]
fn test_exit_code_internal_error() {
    let err: anyhow::Error = SelfwareError::Internal("unexpected state".to_string()).into();
    assert_eq!(
        get_exit_code(&err),
        EXIT_ERROR,
        "Internal error should yield exit code 1"
    );
}

#[test]
fn test_exit_code_plain_anyhow_default() {
    // A plain anyhow error with no recognizable keywords falls back to EXIT_ERROR
    let err = anyhow::anyhow!("something completely unexpected happened");
    assert_eq!(
        get_exit_code(&err),
        EXIT_ERROR,
        "Unrecognized plain anyhow error should yield exit code 1"
    );
}

#[test]
fn test_exit_code_string_fallback_config() {
    // Plain anyhow with "config" in the message triggers string fallback
    let err = anyhow::anyhow!("config file not found");
    assert_eq!(
        get_exit_code(&err),
        EXIT_CONFIG_ERROR,
        "String containing 'config' should fallback to exit code 2"
    );
}

#[test]
fn test_exit_code_string_fallback_api_error() {
    let err = anyhow::anyhow!("api error: rate limited");
    assert_eq!(
        get_exit_code(&err),
        EXIT_API_ERROR,
        "String containing 'api error' should fallback to exit code 4"
    );
}

#[test]
fn test_exit_code_string_fallback_network() {
    let err = anyhow::anyhow!("network connection refused");
    assert_eq!(
        get_exit_code(&err),
        EXIT_API_ERROR,
        "String containing 'network' should fallback to exit code 4"
    );
}

#[test]
fn test_exit_code_string_fallback_safety() {
    let err = anyhow::anyhow!("safety violation detected");
    assert_eq!(
        get_exit_code(&err),
        EXIT_SAFETY_ERROR,
        "String containing 'safety' should fallback to exit code 5"
    );
}

#[test]
fn test_exit_code_string_fallback_blocked() {
    let err = anyhow::anyhow!("operation blocked by policy");
    assert_eq!(
        get_exit_code(&err),
        EXIT_SAFETY_ERROR,
        "String containing 'blocked' should fallback to exit code 5"
    );
}

#[test]
fn test_exit_code_constants() {
    assert_eq!(EXIT_SUCCESS, 0);
    assert_eq!(EXIT_ERROR, 1);
    assert_eq!(EXIT_CONFIG_ERROR, 2);
    assert_eq!(EXIT_API_ERROR, 4);
    assert_eq!(EXIT_SAFETY_ERROR, 5);
    assert_eq!(EXIT_CONFIRMATION_REQUIRED, 6);
}
