use super::*;

#[test]
fn test_default_config() {
    let cfg = LongRunningConfig::default();
    assert_eq!(cfg.endpoint, "http://127.0.0.1:1234/v1");
    assert_eq!(cfg.model, "qwen3.5-27b");
    assert_eq!(cfg.max_iterations, 80);
    assert_eq!(cfg.timeout_per_project_secs, 900);
}

#[test]
fn test_config_new() {
    let cfg = LongRunningConfig::new("http://example.com/v1", "test-model");
    assert_eq!(cfg.endpoint, "http://example.com/v1");
    assert_eq!(cfg.model, "test-model");
}

#[test]
fn to_selfware_toml_is_valid_and_round_trips_extra_body() {
    // Regression: the default extra_body was emitted as pretty JSON under a
    // [extra_body] header — invalid TOML, so every spawned child died at
    // config parse and long-test was broken out of the box.
    let cfg = LongRunningConfig::default();
    let toml_str = cfg.to_selfware_toml();

    // Must now parse as valid TOML.
    let parsed: toml::Value = toml::from_str(&toml_str)
        .unwrap_or_else(|e| panic!("generated config is not valid TOML: {e}\n---\n{toml_str}"));

    // extra_body round-trips as a nested table.
    let enable_thinking = parsed
        .get("extra_body")
        .and_then(|v| v.get("chat_template_kwargs"))
        .and_then(|v| v.get("enable_thinking"))
        .and_then(|v| v.as_bool());
    assert_eq!(
        enable_thinking,
        Some(false),
        "extra_body nesting lost:\n{toml_str}"
    );

    // And selfware's own config loader accepts the generated config.
    let loaded: std::result::Result<crate::config::Config, _> = toml::from_str(&toml_str);
    assert!(
        loaded.is_ok(),
        "selfware rejected the generated config: {:?}\n---\n{toml_str}",
        loaded.err()
    );
}

#[test]
fn to_selfware_toml_handles_empty_extra_body() {
    let cfg = LongRunningConfig {
        extra_body: serde_json::json!({}),
        ..Default::default()
    };
    let toml_str = cfg.to_selfware_toml();
    let parsed: std::result::Result<toml::Value, _> = toml::from_str(&toml_str);
    assert!(parsed.is_ok(), "empty extra_body must still be valid TOML");
}

#[test]
fn test_validate_empty_endpoint() {
    let cfg = LongRunningConfig {
        endpoint: String::new(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn test_toml_generation() {
    let cfg = LongRunningConfig::default();
    let toml = cfg.to_selfware_toml();
    assert!(toml.contains("endpoint = "));
    assert!(toml.contains("model = "));
    assert!(toml.contains("max_iterations = 80"));
}
