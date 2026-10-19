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

#[test]
fn test_local_first_invalidation_and_clear() {
    let mut coord = LocalFirstCoordinator::new();
    coord.cache_response("file_read:{\"path\":\"src/lib.rs\"}", "lib".to_string(), 3);
    coord.cache_response(
        "file_read:{\"path\":\"README.md\"}",
        "readme".to_string(),
        6,
    );
    coord.cache_response("grep_search:{\"pattern\":\"fn\"}", "hits".to_string(), 4);
    coord.cache_response("git_status:{}", "clean".to_string(), 5);

    coord.invalidate_git_and_path("src/lib.rs");
    assert!(coord
        .get_response("file_read:{\"path\":\"src/lib.rs\"}")
        .is_none());
    assert!(coord
        .get_response("grep_search:{\"pattern\":\"fn\"}")
        .is_none());
    assert!(coord.get_response("git_status:{}").is_none());
    assert_eq!(
        coord.get_response("file_read:{\"path\":\"README.md\"}"),
        Some("readme")
    );

    coord.clear();
    assert_eq!(coord.stats().cache_stats.entry_count, 0);
}
