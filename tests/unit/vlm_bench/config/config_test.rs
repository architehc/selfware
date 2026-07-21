use super::*;

#[test]
fn test_default_config() {
    let cfg = VlmBenchConfig::default();
    assert_eq!(cfg.endpoint, "http://192.168.1.99:1234/v1");
    assert_eq!(cfg.model, "qwen/qwen3.5-9b");
    assert_eq!(cfg.max_concurrent, 4);
    assert_eq!(cfg.max_tokens, 4096);
    assert!((cfg.temperature - 0.2).abs() < f32::EPSILON);
    assert_eq!(cfg.timeout_secs, 120);
    assert_eq!(cfg.levels.len(), 6);
}

#[test]
fn test_config_new() {
    let cfg = VlmBenchConfig::new("http://localhost:8080/v1", "test-model");
    assert_eq!(cfg.endpoint, "http://localhost:8080/v1");
    assert_eq!(cfg.model, "test-model");
    assert_eq!(cfg.max_concurrent, 4); // default
}

#[test]
fn test_with_max_difficulty() {
    let cfg = VlmBenchConfig::default().with_max_difficulty(Difficulty::Medium);
    assert_eq!(cfg.levels.len(), 2);
    assert!(cfg.levels.contains(&Difficulty::Easy));
    assert!(cfg.levels.contains(&Difficulty::Medium));
    assert!(!cfg.levels.contains(&Difficulty::Hard));
}

#[test]
fn test_with_concurrency() {
    let cfg = VlmBenchConfig::default().with_concurrency(8);
    assert_eq!(cfg.max_concurrent, 8);

    let cfg = VlmBenchConfig::default().with_concurrency(0);
    assert_eq!(cfg.max_concurrent, 1); // clamped
}

#[test]
fn test_validate_ok() {
    assert!(VlmBenchConfig::default().validate().is_ok());
}

#[test]
fn test_validate_empty_endpoint() {
    let mut cfg = VlmBenchConfig::default();
    cfg.endpoint = String::new();
    assert!(cfg.validate().is_err());
}

#[test]
fn test_validate_empty_model() {
    let mut cfg = VlmBenchConfig::default();
    cfg.model = String::new();
    assert!(cfg.validate().is_err());
}

#[test]
fn test_validate_zero_concurrent() {
    let mut cfg = VlmBenchConfig::default();
    cfg.max_concurrent = 0;
    assert!(cfg.validate().is_err());
}

#[test]
fn test_validate_empty_levels() {
    let mut cfg = VlmBenchConfig::default();
    cfg.levels.clear();
    assert!(cfg.validate().is_err());
}

#[test]
fn test_config_serde_roundtrip() {
    let cfg = VlmBenchConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: VlmBenchConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.endpoint, cfg.endpoint);
    assert_eq!(parsed.model, cfg.model);
    assert_eq!(parsed.levels.len(), cfg.levels.len());
}
