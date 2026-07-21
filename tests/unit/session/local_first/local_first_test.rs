use super::*;

#[test]
fn test_local_first_coordinator_new() {
    let coord = LocalFirstCoordinator::new();
    let stats = coord.stats();
    assert_eq!(stats.cache_stats.entry_count, 0);
    assert_eq!(stats.offline_status, OfflineStatus::Online);
}

#[test]
fn test_cache_response() {
    let mut coord = LocalFirstCoordinator::new();
    coord.cache_response("key1", "value1".to_string(), 6);

    let stats = coord.stats();
    assert_eq!(stats.cache_stats.entry_count, 1);
    assert_eq!(stats.bandwidth_saved_bytes, 6);
}

#[test]
fn test_local_cache_basic() {
    let mut cache = LocalCache::new();
    let entry = LocalCacheEntry {
        key: "test".to_string(),
        value: "value".to_string(),
        size_bytes: 5,
    };
    cache.put(entry);

    let stats = cache.stats();
    assert_eq!(stats.entry_count, 1);
    assert_eq!(stats.total_size_bytes, 5);
}
