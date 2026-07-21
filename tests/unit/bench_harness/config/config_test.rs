use super::*;

#[test]
fn test_default_config() {
    let cfg = HarnessConfig::default();
    assert_eq!(cfg.endpoint, "http://127.0.0.1:1234/v1");
    assert_eq!(cfg.model, "qwen3.5-27b");
    assert_eq!(cfg.max_concurrent, 32);
    assert_eq!(cfg.max_tokens, 65536);
    assert!((cfg.temperature - 0.2).abs() < f32::EPSILON);
    assert_eq!(cfg.timeout_secs, 300);
}

#[test]
fn test_config_new() {
    let cfg = HarnessConfig::new("http://example.com/v1", "test-model");
    assert_eq!(cfg.endpoint, "http://example.com/v1");
    assert_eq!(cfg.model, "test-model");
    assert_eq!(cfg.max_concurrent, 32);
}

#[test]
fn test_with_concurrency() {
    let cfg = HarnessConfig::default().with_concurrency(8);
    assert_eq!(cfg.max_concurrent, 8);

    let cfg = HarnessConfig::default().with_concurrency(0);
    assert_eq!(cfg.max_concurrent, 1);
}

#[test]
fn test_validate_ok() {
    assert!(HarnessConfig::default().validate().is_ok());
}

#[test]
fn test_validate_empty_endpoint() {
    let cfg = HarnessConfig {
        endpoint: String::new(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn test_validate_empty_model() {
    let cfg = HarnessConfig {
        model: String::new(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn api_key_is_never_serialized_or_debug_printed() {
    assert!(HarnessConfig::default().api_key.is_none());
    let cfg = HarnessConfig {
        api_key: Some("sk-super-secret".into()),
        ..Default::default()
    };
    // Security: the key must NOT appear in the serialized report artifact.
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(
        !json.contains("sk-super-secret"),
        "api_key leaked into serialized HarnessConfig: {json}"
    );
    assert!(!json.contains("api_key"), "api_key field must be skipped");
    // Security: the key must NOT appear in Debug output either.
    let dbg = format!("{cfg:?}");
    assert!(
        !dbg.contains("sk-super-secret"),
        "api_key leaked into Debug: {dbg}"
    );
    assert!(dbg.contains("<redacted>"), "Debug should mark a set key");
    // Deserializing a report (no api_key field) yields None, not an error.
    let back: HarnessConfig = serde_json::from_str(&json).unwrap();
    assert!(back.api_key.is_none());
}

#[test]
fn test_config_serde_roundtrip() {
    let cfg = HarnessConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: HarnessConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.endpoint, cfg.endpoint);
    assert_eq!(parsed.model, cfg.model);
    assert_eq!(parsed.max_concurrent, cfg.max_concurrent);
}
