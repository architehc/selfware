use super::*;

#[test]
fn test_engine_creation() {
    let config = ConsolidationConfig::default();
    let engine = ConsolidationEngine::new(config);
    assert!(engine.is_ok());
}

#[test]
fn test_engine_with_storage_dir() {
    let config = ConsolidationConfig::default();
    let engine = ConsolidationEngine::new(config)
        .unwrap()
        .with_storage_dir(PathBuf::from("/tmp/test_consolidation"));
    // Just verify it doesn't panic
    let _ = engine;
}

#[tokio::test]
async fn test_consolidate_empty() {
    let config = ConsolidationConfig::default();
    let mut engine = ConsolidationEngine::new(config).unwrap();
    let report = engine.consolidate_episodes(vec![]).await.unwrap();
    assert_eq!(report.episodes_processed, 0);
    assert_eq!(report.records_produced, 0);
}
