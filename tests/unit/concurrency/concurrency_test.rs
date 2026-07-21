use super::*;

#[tokio::test]
async fn test_governor_limits_concurrent_operations() {
    let gov = ConcurrencyGovernor::new(2, 2, 4);

    // Acquire two tool permits — should succeed
    let p1 = gov.acquire_tool().await.unwrap();
    let p2 = gov.acquire_tool().await.unwrap();

    // Third acquire should not succeed via try_acquire (at capacity)
    assert!(gov.try_acquire_tool().is_none());

    // Drop one and retry
    drop(p1);
    let p3 = gov.try_acquire_tool();
    assert!(p3.is_some());

    drop(p2);
    drop(p3);
}

#[tokio::test]
async fn test_permits_released_on_drop() {
    let gov = ConcurrencyGovernor::new(1, 1, 2);

    {
        let _permit = gov.acquire_tool().await.unwrap();
        assert_eq!(gov.stats().tools_available, 0);
        assert_eq!(gov.stats().global_available, 1);
    }
    // After drop, permits should be available again
    assert_eq!(gov.stats().tools_available, 1);
    assert_eq!(gov.stats().global_available, 2);
}

#[tokio::test]
async fn test_try_acquire_returns_none_at_capacity() {
    let gov = ConcurrencyGovernor::new(1, 1, 1);

    let _permit = gov.acquire_tool().await.unwrap();
    assert!(gov.try_acquire_tool().is_none());
}

#[tokio::test]
async fn test_stats_report_correct_values() {
    let gov = ConcurrencyGovernor::new(4, 8, 16);

    let stats = gov.stats();
    assert_eq!(stats.streams_available, 4);
    assert_eq!(stats.streams_max, 4);
    assert_eq!(stats.tools_available, 8);
    assert_eq!(stats.tools_max, 8);
    assert_eq!(stats.global_available, 16);
    assert_eq!(stats.global_max, 16);

    // Acquire one stream permit
    let _s = gov.acquire_stream().await.unwrap();
    let stats = gov.stats();
    assert_eq!(stats.streams_available, 3);
    assert_eq!(stats.global_available, 15);

    // Acquire one tool permit
    let _t = gov.acquire_tool().await.unwrap();
    let stats = gov.stats();
    assert_eq!(stats.tools_available, 7);
    assert_eq!(stats.global_available, 14);
}

#[tokio::test]
async fn test_global_limit_caps_total_operations() {
    // Global limit is 2, but stream and tool limits are higher
    let gov = ConcurrencyGovernor::new(4, 4, 2);

    let _p1 = gov.acquire_tool().await.unwrap();
    let _p2 = gov.acquire_stream().await.unwrap();

    // Global is now exhausted — try_acquire should fail
    assert!(gov.try_acquire_tool().is_none());
}

#[tokio::test]
async fn test_default_values() {
    let gov = ConcurrencyGovernor::with_defaults();
    let stats = gov.stats();
    assert_eq!(stats.streams_max, 4);
    assert_eq!(stats.tools_max, 8);
    assert_eq!(stats.global_max, 12);
}
