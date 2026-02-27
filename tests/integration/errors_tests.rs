use selfware::errors::{
    get_exit_code, is_confirmation_error, AgentError, ApiError, ResourceError, SafetyError,
    SelfwareError, EXIT_API_ERROR, EXIT_CONFIG_ERROR, EXIT_CONFIRMATION_REQUIRED, EXIT_ERROR,
    EXIT_SAFETY_ERROR,
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
