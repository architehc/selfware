use super::*;

#[test]
fn test_default_config() {
    let cfg = ConsolidationConfig::default();
    assert_eq!(cfg.interval_secs, 3600);
    assert_eq!(cfg.max_concurrent_llm, 32);
    assert_eq!(cfg.batch_size, 100);
    assert!((cfg.decay_half_life_hours - 24.0).abs() < f64::EPSILON);
}

#[test]
fn test_validate_ok() {
    assert!(ConsolidationConfig::default().validate().is_ok());
}

#[test]
fn test_validate_empty_endpoint() {
    let cfg = ConsolidationConfig {
        endpoint: String::new(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn test_serde_roundtrip() {
    let cfg = ConsolidationConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: ConsolidationConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.endpoint, cfg.endpoint);
    assert_eq!(parsed.max_concurrent_llm, cfg.max_concurrent_llm);
}
