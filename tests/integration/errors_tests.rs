use selfware::errors::{
    get_exit_code, is_confirmation_error, AgentError, ApiError, ResourceError, SafetyError,
    SelfwareError, SessionError, ToolError, EXIT_API_ERROR, EXIT_CONFIG_ERROR,
    EXIT_CONFIRMATION_REQUIRED, EXIT_ERROR, EXIT_SAFETY_ERROR, EXIT_SUCCESS,
};

#[test]
fn test_is_confirmation_error() {
    let agent_err = AgentError::ConfirmationRequired {
        tool_name: "test".to_string(),
    };
    let wrapped = anyhow::Error::from(SelfwareError::Agent(agent_err));
    assert!(is_confirmation_error(&wrapped));

    let other_err = anyhow::anyhow!("some other error");
    assert!(!is_confirmation_error(&other_err));
}

#[test]
fn test_get_exit_code() {
    let config_err = anyhow::Error::from(SelfwareError::Config("bad config".to_string()));
    assert_eq!(get_exit_code(&config_err), EXIT_CONFIG_ERROR);

    let api_err = anyhow::Error::from(SelfwareError::Api(ApiError::Timeout));
    assert_eq!(get_exit_code(&api_err), EXIT_API_ERROR);

    let safety_err = anyhow::Error::from(SelfwareError::Safety(SafetyError::PathTraversal {
        path: ".".to_string(),
    }));
    assert_eq!(get_exit_code(&safety_err), EXIT_SAFETY_ERROR);

    let confirm_err = anyhow::Error::from(SelfwareError::Agent(AgentError::ConfirmationRequired {
        tool_name: "cmd".to_string(),
    }));
    assert_eq!(get_exit_code(&confirm_err), EXIT_CONFIRMATION_REQUIRED);

    let gen_err = anyhow::anyhow!("generic runtime failure");
    assert_eq!(get_exit_code(&gen_err), EXIT_ERROR);

    // Direct unwraps test
    let direct_api = anyhow::Error::from(ApiError::Timeout);
    assert_eq!(get_exit_code(&direct_api), EXIT_API_ERROR);

    // String matching fallback
    let string_config = anyhow::anyhow!("Config file is malformed");
    assert_eq!(get_exit_code(&string_config), EXIT_CONFIG_ERROR);
}

#[test]
fn test_error_display() {
    let err = SelfwareError::Resource(ResourceError::MemoryExhausted("OOM".to_string()));
    assert!(err.to_string().contains("Resource error"));
    assert!(err.to_string().contains("Memory exhausted"));
}

// =========================================================================
// Additional error display message tests
// =========================================================================

#[test]
fn test_all_agent_error_display_messages() {
    let err = AgentError::ConfirmationRequired {
        tool_name: "shell_exec".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("shell_exec"));
    assert!(msg.contains("requires confirmation"));

    let err = AgentError::IterationLimit { limit: 50 };
    assert_eq!(err.to_string(), "Iteration limit reached (50)");

    let err = AgentError::StepTimeout { seconds: 120 };
    assert_eq!(err.to_string(), "Step timeout after 120 seconds");

    let err = AgentError::InvalidStateTransition {
        from: "Planning".to_string(),
        to: "Completed".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("Planning"));
    assert!(msg.contains("Completed"));

    let err = AgentError::Cancelled;
    assert_eq!(err.to_string(), "Task cancelled by user");

    let err = AgentError::MissingSystemPrompt;
    assert_eq!(err.to_string(), "Missing system prompt");

    let err = AgentError::Panic("thread panicked".to_string());
    assert!(err.to_string().contains("thread panicked"));
}

#[test]
fn test_all_api_error_display_messages() {
    assert_eq!(ApiError::Timeout.to_string(), "API Request timed out");

    let err = ApiError::RateLimit {
        retry_after_secs: Some(30),
    };
    assert!(err.to_string().contains("Rate limit"));
    assert!(err.to_string().contains("30"));

    let err = ApiError::RateLimit {
        retry_after_secs: None,
    };
    assert!(err.to_string().contains("Rate limit"));

    let err = ApiError::Authentication("invalid key".to_string());
    assert!(err.to_string().contains("Authentication failed"));
    assert!(err.to_string().contains("invalid key"));

    let err = ApiError::HttpStatus {
        status: 500,
        message: "Internal Server Error".to_string(),
    };
    assert!(err.to_string().contains("500"));
    assert!(err.to_string().contains("Internal Server Error"));

    let err = ApiError::Parse("unexpected token".to_string());
    assert!(err.to_string().contains("parse"));
    assert!(err.to_string().contains("unexpected token"));

    let err = ApiError::Network("connection refused".to_string());
    assert!(err.to_string().contains("Network"));
    assert!(err.to_string().contains("connection refused"));

    let err = ApiError::ModelNotFound("gpt-5-turbo".to_string());
    assert!(err.to_string().contains("Model not found"));
    assert!(err.to_string().contains("gpt-5-turbo"));
}

#[test]
fn test_all_tool_error_display_messages() {
    let err = ToolError::Execution {
        name: "file_write".to_string(),
        message: "permission denied".to_string(),
    };
    assert!(err.to_string().contains("file_write"));
    assert!(err.to_string().contains("permission denied"));

    let err = ToolError::NotFound {
        name: "magic_tool".to_string(),
    };
    assert!(err.to_string().contains("magic_tool"));
    assert!(err.to_string().contains("not found"));

    let err = ToolError::InvalidArguments {
        name: "file_edit".to_string(),
        message: "missing path".to_string(),
    };
    assert!(err.to_string().contains("file_edit"));
    assert!(err.to_string().contains("missing path"));

    let err = ToolError::Timeout;
    assert!(err.to_string().contains("timed out"));
}

#[test]
fn test_all_safety_error_display_messages() {
    let err = SafetyError::BlockedPath {
        path: "/etc/shadow".to_string(),
    };
    assert!(err.to_string().contains("/etc/shadow"));
    assert!(err.to_string().contains("blocked"));

    let err = SafetyError::BlockedCommand {
        command: "rm -rf /".to_string(),
        reason: "destructive".to_string(),
    };
    assert!(err.to_string().contains("rm -rf /"));
    assert!(err.to_string().contains("destructive"));

    let err = SafetyError::SecretDetected {
        finding: "AWS_SECRET_KEY".to_string(),
    };
    assert!(err.to_string().contains("secret"));
    assert!(err.to_string().contains("AWS_SECRET_KEY"));

    let err = SafetyError::ConfirmationRequired {
        action: "delete database".to_string(),
    };
    assert!(err.to_string().contains("confirmation"));
    assert!(err.to_string().contains("delete database"));

    let err = SafetyError::PathTraversal {
        path: "../../etc/passwd".to_string(),
    };
    assert!(err.to_string().contains("traversal"));
    assert!(err.to_string().contains("../../etc/passwd"));
}

#[test]
fn test_all_session_error_display_messages() {
    let err = SessionError::CheckpointSave("disk full".to_string());
    assert!(err.to_string().contains("save checkpoint"));
    assert!(err.to_string().contains("disk full"));

    let err = SessionError::CheckpointLoad("file not found".to_string());
    assert!(err.to_string().contains("load checkpoint"));
    assert!(err.to_string().contains("file not found"));

    let err = SessionError::Storage {
        path: std::path::PathBuf::from("/tmp/data.json"),
        message: "corrupted".to_string(),
    };
    assert!(err.to_string().contains("/tmp/data.json"));
    assert!(err.to_string().contains("corrupted"));

    let err = SessionError::HistoryCorrupted("bad CRC".to_string());
    assert!(err.to_string().contains("corrupted"));
    assert!(err.to_string().contains("bad CRC"));
}

#[test]
fn test_all_resource_error_display_messages() {
    let err = ResourceError::MemoryExhausted("OOM killed".to_string());
    assert!(err.to_string().contains("Memory exhausted"));

    let err = ResourceError::Gpu("CUDA out of memory".to_string());
    assert!(err.to_string().contains("GPU"));
    assert!(err.to_string().contains("CUDA out of memory"));

    let err = ResourceError::DiskExhausted("no space left".to_string());
    assert!(err.to_string().contains("Disk exhausted"));

    let err = ResourceError::QuotaExceeded {
        resource: "tokens".to_string(),
        used: 150_000,
        limit: 100_000,
    };
    let msg = err.to_string();
    assert!(msg.contains("tokens"));
    assert!(msg.contains("150000"));
    assert!(msg.contains("100000"));

    let err = ResourceError::Unavailable("GPU not detected".to_string());
    assert!(err.to_string().contains("unavailable"));
    assert!(err.to_string().contains("GPU not detected"));
}

// =========================================================================
// Error conversion chain tests (From impls)
// =========================================================================

#[test]
fn test_error_conversion_chains() {
    // AgentError -> SelfwareError
    let agent_err = AgentError::Cancelled;
    let selfware_err: SelfwareError = agent_err.into();
    assert!(matches!(
        selfware_err,
        SelfwareError::Agent(AgentError::Cancelled)
    ));

    // ApiError -> SelfwareError
    let api_err = ApiError::Timeout;
    let selfware_err: SelfwareError = api_err.into();
    assert!(matches!(
        selfware_err,
        SelfwareError::Api(ApiError::Timeout)
    ));

    // ToolError -> SelfwareError
    let tool_err = ToolError::Timeout;
    let selfware_err: SelfwareError = tool_err.into();
    assert!(matches!(
        selfware_err,
        SelfwareError::Tool(ToolError::Timeout)
    ));

    // SafetyError -> SelfwareError
    let safety_err = SafetyError::PathTraversal {
        path: "x".to_string(),
    };
    let selfware_err: SelfwareError = safety_err.into();
    assert!(matches!(
        selfware_err,
        SelfwareError::Safety(SafetyError::PathTraversal { .. })
    ));

    // SessionError -> SelfwareError
    let session_err = SessionError::HistoryCorrupted("bad".to_string());
    let selfware_err: SelfwareError = session_err.into();
    assert!(matches!(
        selfware_err,
        SelfwareError::Session(SessionError::HistoryCorrupted(_))
    ));

    // ResourceError -> SelfwareError
    let resource_err = ResourceError::Unavailable("gone".to_string());
    let selfware_err: SelfwareError = resource_err.into();
    assert!(matches!(
        selfware_err,
        SelfwareError::Resource(ResourceError::Unavailable(_))
    ));
}

// =========================================================================
// Exit code mapping tests for all error paths
// =========================================================================

#[test]
fn test_exit_code_for_direct_safety_error() {
    let direct_safety = anyhow::Error::from(SafetyError::BlockedCommand {
        command: "rm".to_string(),
        reason: "dangerous".to_string(),
    });
    assert_eq!(get_exit_code(&direct_safety), EXIT_SAFETY_ERROR);
}

#[test]
fn test_exit_code_for_direct_agent_error_non_confirmation() {
    let direct_agent = anyhow::Error::from(AgentError::Cancelled);
    assert_eq!(get_exit_code(&direct_agent), EXIT_ERROR);
}

#[test]
fn test_exit_code_string_fallback_api_error() {
    let api_like = anyhow::anyhow!("api error: connection dropped");
    assert_eq!(get_exit_code(&api_like), EXIT_API_ERROR);
}

#[test]
fn test_exit_code_string_fallback_network() {
    let network_like = anyhow::anyhow!("network timeout occurred");
    assert_eq!(get_exit_code(&network_like), EXIT_API_ERROR);
}

#[test]
fn test_exit_code_string_fallback_safety() {
    let safety_like = anyhow::anyhow!("safety violation: blocked path");
    assert_eq!(get_exit_code(&safety_like), EXIT_SAFETY_ERROR);
}

#[test]
fn test_exit_code_string_fallback_blocked() {
    let blocked_like = anyhow::anyhow!("operation blocked by policy");
    assert_eq!(get_exit_code(&blocked_like), EXIT_SAFETY_ERROR);
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

#[test]
fn test_selfware_error_display_wrapping() {
    // SelfwareError wraps inner errors and prefixes them
    let err = SelfwareError::Config("missing API key".to_string());
    assert_eq!(err.to_string(), "Configuration error: missing API key");

    let err = SelfwareError::Internal("unexpected state".to_string());
    assert_eq!(err.to_string(), "Internal error: unexpected state");

    let err = SelfwareError::Agent(AgentError::Cancelled);
    assert_eq!(err.to_string(), "Agent error: Task cancelled by user");

    let err = SelfwareError::Api(ApiError::Timeout);
    assert_eq!(err.to_string(), "API error: API Request timed out");

    let err = SelfwareError::Tool(ToolError::Timeout);
    assert_eq!(err.to_string(), "Tool error: Tool execution timed out");

    let err = SelfwareError::Safety(SafetyError::PathTraversal {
        path: "../secret".to_string(),
    });
    assert!(err.to_string().starts_with("Safety error:"));

    let err = SelfwareError::Session(SessionError::CheckpointSave("io".to_string()));
    assert!(err.to_string().starts_with("Session error:"));

    let err = SelfwareError::Resource(ResourceError::Gpu("no GPU".to_string()));
    assert!(err.to_string().starts_with("Resource error:"));
}
