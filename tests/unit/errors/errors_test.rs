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

// =========================================================================
// Shutdown-reason → typed cancellation error (one shared mapping)
// =========================================================================

#[test]
fn shutdown_reason_maps_sigterm_to_terminated_exit_143() {
    let err = AgentError::from_shutdown_reason(Some(crate::ShutdownReason::SignalTerminate));
    assert!(matches!(err, AgentError::Terminated(ref s) if s == "SIGTERM"));
    assert_eq!(get_exit_code(&anyhow::Error::from(err)), 143);
}

#[test]
fn shutdown_reason_maps_internal_timeout_to_cancelled_with_reason() {
    let err = AgentError::from_shutdown_reason(Some(crate::ShutdownReason::Timeout));
    assert!(matches!(err, AgentError::CancelledWithReason(ref s) if s == "timeout"));
    assert_eq!(get_exit_code(&anyhow::Error::from(err)), EXIT_INTERRUPTED);
}

#[test]
fn only_user_interrupt_or_no_latch_maps_to_bare_cancelled() {
    for reason in [Some(crate::ShutdownReason::UserInterrupt), None] {
        let err = AgentError::from_shutdown_reason(reason);
        assert!(
            matches!(err, AgentError::Cancelled),
            "{reason:?} must be a user cancel"
        );
        assert_eq!(err.to_string(), "Task cancelled by user");
    }
}

// =========================================================================
// Provider context-overflow classification
// =========================================================================

/// Real-shaped provider bodies for "the prompt does not fit the window".
const OVERFLOW_BODIES: &[(u16, &str)] = &[
    // OpenAI / DeepSeek
    (
        400,
        r#"{"error":{"message":"This model's maximum context length is 8192 tokens. However, your messages resulted in 9000 tokens (including 200 in the functions). Please reduce the length of the messages or functions.","type":"invalid_request_error","param":"messages","code":"context_length_exceeded"}}"#,
    ),
    // OpenAI newer wording
    (
        400,
        r#"{"error":{"message":"Your input exceeds the context window of this model. Please adjust your input and try again.","code":"context_length_exceeded"}}"#,
    ),
    // vLLM
    (
        400,
        r#"{"object":"error","message":"This model's maximum context length is 32768 tokens. However, you requested 40035 tokens (39035 in the messages, 1000 in the completion). Please reduce the length of the messages or completion.","type":"BadRequestError","code":400}"#,
    ),
    // SGLang
    (
        400,
        r#"{"object":"error","message":"The input (40000 tokens) is longer than the model's context length (32768 tokens).","type":"BadRequestError","code":400}"#,
    ),
    // llama.cpp server
    (
        400,
        r#"{"error":{"code":400,"message":"the request exceeds the available context size, try increasing it","type":"exceed_context_size_error","n_prompt_tokens":9000,"n_ctx":8192}}"#,
    ),
    // Ollama
    (
        400,
        r#"{"error":"input length exceeds maximum context length"}"#,
    ),
    // OpenRouter
    (
        400,
        r#"{"error":{"message":"This endpoint's maximum context length is 131072 tokens. However, you requested about 140000 tokens (138000 of text input, 2000 in the output). Please reduce the length of either one.","code":400}}"#,
    ),
    // Anthropic-style
    (
        400,
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 210000 tokens > 200000 maximum"}}"#,
    ),
    (
        413,
        r#"{"type":"error","error":{"type":"request_too_large","message":"Request exceeds the maximum allowed number of bytes."}}"#,
    ),
    // TGI
    (
        422,
        r#"{"error":"Input validation error: `inputs` tokens + `max_new_tokens` must be <= 4096. Given: 4000 `inputs` tokens and 512 `max_new_tokens`","error_type":"validation"}"#,
    ),
    // Bare 413 with no body (proxy-level payload limit)
    (413, ""),
];

#[test]
fn provider_context_overflow_bodies_are_classified() {
    for (status, body) in OVERFLOW_BODIES {
        assert!(
            is_provider_context_overflow(*status, body),
            "HTTP {status} must classify as context overflow: {body}"
        );
        let err: anyhow::Error = ApiError::HttpStatus {
            status: *status,
            message: body.to_string(),
        }
        .into();
        assert!(is_context_overflow_error(&err), "{status}: {body}");
    }
}

#[test]
fn genuine_client_errors_are_not_context_overflow() {
    let genuine: &[(u16, &str)] = &[
        (
            400,
            r#"{"error":{"message":"Invalid value for 'temperature': must be <= 2","type":"invalid_request_error"}}"#,
        ),
        (
            400,
            r#"{"error":{"message":"The model `gpt-9` does not exist","code":"model_not_found"}}"#,
        ),
        (
            400,
            r#"{"error":"messages with role 'tool' must immediately follow an assistant message with 'tool_calls'"}"#,
        ),
        (400, ""),
        (
            422,
            r#"{"detail":[{"loc":["body","messages"],"msg":"field required"}]}"#,
        ),
        // Auth / not found / rate limit are never overflow, whatever the body says.
        (401, "maximum context length is 8192 tokens"),
        (403, "prompt is too long"),
        (404, "context_length_exceeded"),
        (
            429,
            "Rate limit reached: too many tokens per minute; context_length_exceeded",
        ),
        (500, "maximum context length"),
    ];
    for (status, body) in genuine {
        assert!(
            !is_provider_context_overflow(*status, body),
            "HTTP {status} must NOT classify as context overflow: {body}"
        );
    }
}

#[test]
fn context_overflow_is_found_through_anyhow_context() {
    let err = anyhow::Error::from(ApiError::ContextOverflow("too big".into()))
        .context("Streaming failed. Non-streaming fallback request also failed");
    assert!(is_context_overflow_error(&err));
    let plain: anyhow::Error = ApiError::Timeout.into();
    assert!(!is_context_overflow_error(&plain));
}

#[test]
fn reasoning_only_long_call_message_names_the_reasoning() {
    let msg = ApiError::ReasoningOnlyLongCall {
        elapsed_ms: 400_000,
        reasoning_chars: 1234,
    }
    .to_string();
    assert!(msg.contains("400000ms"), "{msg}");
    assert!(msg.contains("1234 reasoning chars"), "{msg}");
    assert!(!msg.contains("no reasoning"), "{msg}");
}
