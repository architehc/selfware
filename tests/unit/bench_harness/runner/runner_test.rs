use super::*;

#[test]
fn test_runner_creation() {
    let config = HarnessConfig::default();
    let runner = HarnessRunner::new(config).unwrap();
    assert_eq!(runner.config.max_concurrent, 32);
}

#[test]
fn test_runner_invalid_config() {
    let config = HarnessConfig {
        endpoint: String::new(),
        ..Default::default()
    };
    assert!(HarnessRunner::new(config).is_err());
}
