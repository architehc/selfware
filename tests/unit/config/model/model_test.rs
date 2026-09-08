use super::*;

// =========================================================================
// RedactedString tests
// =========================================================================

#[test]
fn test_redacted_string_new() {
    let rs = RedactedString::new("my-secret-key");
    assert_eq!(rs.expose(), "my-secret-key");
}

#[test]
fn test_redacted_string_display_hides_secret() {
    let rs = RedactedString::new("super-secret");
    assert_eq!(format!("{}", rs), "[REDACTED]");
    assert!(!format!("{}", rs).contains("super-secret"));
}

#[test]
fn test_redacted_string_debug_hides_secret() {
    let rs = RedactedString::new("my-api-key");
    assert_eq!(format!("{:?}", rs), "[REDACTED]");
    assert!(!format!("{:?}", rs).contains("my-api-key"));
}

#[test]
fn test_redacted_string_expose() {
    let rs = RedactedString::new("exposed-value");
    assert_eq!(rs.expose(), "exposed-value");
}

#[test]
fn test_redacted_string_eq() {
    let a = RedactedString::new("same");
    let b = RedactedString::new("same");
    assert_eq!(a, b);
}

#[test]
fn test_redacted_string_ne() {
    let a = RedactedString::new("one");
    let b = RedactedString::new("two");
    assert_ne!(a, b);
}

#[test]
fn test_redacted_string_eq_str() {
    let rs = RedactedString::new("hello");
    assert!(rs == *"hello");
    assert!(!(rs == *"world"));
}

#[test]
fn test_redacted_string_from_string() {
    let rs: RedactedString = "test-key".to_string().into();
    assert_eq!(rs.expose(), "test-key");
}

#[test]
fn test_redacted_string_from_str() {
    let rs: RedactedString = "test-key".into();
    assert_eq!(rs.expose(), "test-key");
}

#[test]
fn test_redacted_string_clone() {
    let rs = RedactedString::new("cloneable");
    let cloned = rs.clone();
    assert_eq!(rs, cloned);
    assert_eq!(cloned.expose(), "cloneable");
}

#[test]
fn test_redacted_string_serialize() {
    let rs = RedactedString::new("serialized-value");
    let json = serde_json::to_string(&rs).unwrap();
    assert_eq!(json, "\"serialized-value\"");
}

#[test]
fn test_redacted_string_deserialize() {
    let rs: RedactedString = serde_json::from_str("\"deserialized-value\"").unwrap();
    assert_eq!(rs.expose(), "deserialized-value");
}

#[test]
fn test_redacted_string_serde_roundtrip() {
    let original = RedactedString::new("roundtrip-secret");
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: RedactedString = serde_json::from_str(&json).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_redacted_string_empty() {
    let rs = RedactedString::new("");
    assert_eq!(rs.expose(), "");
    assert_eq!(format!("{}", rs), "[REDACTED]");
}

#[test]
fn test_redacted_string_special_chars() {
    let rs = RedactedString::new("key=abc&token=xyz!@#$%");
    assert_eq!(rs.expose(), "key=abc&token=xyz!@#$%");
}

// =========================================================================
// ModelProfile tests
// =========================================================================

#[test]
fn test_model_profile_supports_vision_true() {
    let profile = ModelProfile {
        endpoint: "http://localhost:8080/v1".to_string(),
        model: "test-model".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.7,
        modalities: vec!["text".to_string(), "vision".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    assert!(profile.supports_vision());
}

#[test]
fn test_model_profile_supports_vision_false() {
    let profile = ModelProfile {
        endpoint: "http://localhost:8080/v1".to_string(),
        model: "test-model".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.7,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    assert!(!profile.supports_vision());
}

#[test]
fn test_model_profile_supports_vision_empty_modalities() {
    let profile = ModelProfile {
        endpoint: "http://localhost:8080/v1".to_string(),
        model: "test-model".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.7,
        modalities: vec![],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    assert!(!profile.supports_vision());
}

#[test]
fn test_model_profile_serialization() {
    let profile = ModelProfile {
        endpoint: "http://localhost:8080/v1".to_string(),
        model: "qwen-72b".to_string(),
        api_key: Some(RedactedString::new("sk-test")),
        max_tokens: 8192,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 131072,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    let json = serde_json::to_string(&profile).unwrap();
    assert!(json.contains("qwen-72b"));
    assert!(json.contains("131072"));
    // api_key should serialize as plain string (not redacted)
    assert!(json.contains("sk-test"));
}

#[test]
fn test_model_profile_deserialization() {
    let json = r#"{
            "endpoint": "http://example.com/v1",
            "model": "test-model",
            "api_key": "my-key",
            "max_tokens": 2048,
            "temperature": 0.3,
            "modalities": ["text", "vision"],
            "context_length": 65536
        }"#;
    let profile: ModelProfile = serde_json::from_str(json).unwrap();
    assert_eq!(profile.endpoint, "http://example.com/v1");
    assert_eq!(profile.model, "test-model");
    assert_eq!(profile.api_key.as_ref().unwrap().expose(), "my-key");
    assert_eq!(profile.max_tokens, 2048);
    assert_eq!(profile.temperature, 0.3);
    assert!(profile.supports_vision());
    assert_eq!(profile.context_length, 65536);
}

#[test]
fn test_model_profile_with_extra_body() {
    let mut extra = serde_json::Map::new();
    extra.insert("top_p".to_string(), serde_json::json!(0.95));
    extra.insert("repetition_penalty".to_string(), serde_json::json!(1.1));

    let profile = ModelProfile {
        endpoint: "http://localhost/v1".to_string(),
        model: "test".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.7,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: Some(extra),
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    let extra = profile.extra_body.as_ref().unwrap();
    assert_eq!(extra["top_p"], serde_json::json!(0.95));
}

#[test]
fn test_default_modalities_returns_text() {
    let mods = default_modalities();
    assert_eq!(mods, vec!["text".to_string()]);
}

// =========================================================================
// redact_config_secrets tests
// =========================================================================

#[test]
fn test_redact_config_secrets_api_key_top_level_and_profiles() {
    let mut value = serde_json::json!({
        "endpoint": "https://openrouter.ai/api/v1",
        "api_key": "sk-top-level-secret",
        "models": {
            "default": {
                "endpoint": "http://localhost:1234/v1",
                "api_key": "sk-profile-secret"
            },
            "vision": {
                "endpoint": "http://localhost:9999/v1",
                "api_key": null
            }
        }
    });
    redact_config_secrets(&mut value);
    let s = value.to_string();
    assert!(!s.contains("sk-top-level-secret"));
    assert!(!s.contains("sk-profile-secret"));
    assert_eq!(value["api_key"], REDACTED_SECRET_MARKER);
    assert_eq!(
        value["models"]["default"]["api_key"],
        REDACTED_SECRET_MARKER
    );
    // A null api_key stays null (nothing to redact).
    assert!(value["models"]["vision"]["api_key"].is_null());
    // Non-secret fields are untouched.
    assert_eq!(value["endpoint"], "https://openrouter.ai/api/v1");
}

#[test]
fn test_redact_config_secrets_mcp_server_env() {
    let mut value = serde_json::json!({
        "mcp": {
            "servers": [
                {
                    "name": "github",
                    "command": "npx",
                    "env": { "GITHUB_TOKEN": "ghp-secret", "OTHER": "x" }
                }
            ]
        }
    });
    redact_config_secrets(&mut value);
    let s = value.to_string();
    assert!(!s.contains("ghp-secret"));
    assert_eq!(
        value["mcp"]["servers"][0]["env"]["GITHUB_TOKEN"],
        REDACTED_SECRET_MARKER
    );
    assert_eq!(
        value["mcp"]["servers"][0]["env"]["OTHER"],
        REDACTED_SECRET_MARKER
    );
    assert_eq!(value["mcp"]["servers"][0]["command"], "npx");
}

#[test]
fn test_redact_config_secrets_recurses_arrays() {
    let mut value = serde_json::json!({
        "nested": [ {"api_key": "sk-deep"}, [ {"api_key": "sk-deeper"} ] ]
    });
    redact_config_secrets(&mut value);
    let s = value.to_string();
    assert!(!s.contains("sk-deep"));
    assert!(!s.contains("sk-deeper"));
}

#[test]
fn test_redact_config_secrets_no_secrets_is_noop() {
    let mut value = serde_json::json!({"endpoint": "http://x", "max_tokens": 4096});
    let before = value.clone();
    redact_config_secrets(&mut value);
    assert_eq!(value, before);
}

#[test]
fn test_model_profile_clone() {
    let profile = ModelProfile {
        endpoint: "http://localhost/v1".to_string(),
        model: "model".to_string(),
        api_key: Some(RedactedString::new("key")),
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 8192,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    let cloned = profile.clone();
    assert_eq!(cloned.model, "model");
    assert_eq!(cloned.api_key.as_ref().unwrap().expose(), "key");
}

#[test]
fn test_profile_max_retries_override_resolution() {
    let base = ModelProfile {
        endpoint: "http://localhost/v1".to_string(),
        model: "m".to_string(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.0,
        modalities: vec!["text".to_string()],
        context_length: 8192,
        extra_body: None,
        native_function_calling: None,
        max_retries: None,
        response_timeout_floor_secs: None,
    };
    // None inherits the parent default.
    assert_eq!(base.effective_max_retries(5), 5);
    // Some wins over the parent default.
    let overridden = ModelProfile {
        max_retries: Some(9),
        ..base.clone()
    };
    assert_eq!(overridden.effective_max_retries(5), 9);
    // Zero is a legitimate explicit choice (fail fast on flaky local boxes).
    let fail_fast = ModelProfile {
        max_retries: Some(0),
        ..base
    };
    assert_eq!(fail_fast.effective_max_retries(5), 0);
}

#[test]
fn test_profile_response_timeout_floor_deserialization() {
    let toml_src = r#"
        endpoint = "http://localhost:31000/v1"
        model = "local-model"
        response_timeout_floor_secs = 1800
        max_retries = 2
    "#;
    let profile: ModelProfile = toml::from_str(toml_src).unwrap();
    assert_eq!(profile.response_timeout_floor_secs, Some(1800));
    assert_eq!(profile.max_retries, Some(2));
    // Absent fields stay None (backwards compatible with existing configs).
    let minimal: ModelProfile =
        toml::from_str("endpoint = \"http://localhost/v1\"\nmodel = \"m\"").unwrap();
    assert_eq!(minimal.response_timeout_floor_secs, None);
    assert_eq!(minimal.max_retries, None);
}
