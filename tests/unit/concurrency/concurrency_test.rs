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

#[tokio::test]
async fn test_concurrency_governor_16_slot_server_cap_queues_17th() {
    // SGLang endpoint deployment has a 16-slot concurrency limit.
    // The profile pins max_streams to 16 AND max_global to 16 alongside it,
    // so acquire_stream (which holds both a stream permit and a global permit)
    // is not capped by the default max_global of 12.
    let mut config = crate::config::Config {
        model: "qwen38-flash-next".to_string(),
        ..Default::default()
    };
    let profile =
        crate::config::model_profiles::match_profile(&config.model).expect("profile must match");
    let user_explicit = crate::config::model_profiles::UserExplicitFields::default();
    crate::config::model_profiles::apply_profile(&mut config, &profile, &user_explicit);

    assert_eq!(config.concurrency.max_streams, 16);
    assert_eq!(config.concurrency.max_global, 24);

    let gov = ConcurrencyGovernor::from_config(&config.concurrency);

    let mut permits = Vec::new();
    for _ in 0..16 {
        permits.push(
            gov.acquire_stream()
                .await
                .expect("slot within 16 must be granted"),
        );
    }

    assert_eq!(gov.stats().streams_available, 0);
    assert_eq!(gov.stats().global_available, 8);

    // 17th stream request must wait / queue when all 16 slots are held
    let timeout_result =
        tokio::time::timeout(std::time::Duration::from_millis(50), gov.acquire_stream()).await;
    assert!(
        timeout_result.is_err(),
        "17th stream must queue while 16 slots are held"
    );

    // Once a permit drops, the 17th request acquires successfully
    drop(permits.pop());
    assert_eq!(gov.stats().streams_available, 1);
    let p17 =
        tokio::time::timeout(std::time::Duration::from_millis(50), gov.acquire_stream()).await;
    assert!(p17.is_ok(), "17th stream acquires once a permit frees up");
}

/// A backlog of stream requesters must not consume the global budget, or the
/// tool category starves while its own semaphore sits completely idle.
///
/// This pins the acquisition order: with global-first, each parked stream
/// waiter holds a global permit, so `max_global` exhausts on *waiters* and an
/// `acquire_tool` that has tool slots to spare queues behind a stream that has
/// not even started. Category-first is what makes the global limit a ceiling
/// on admitted work rather than a reservation parked on by whoever queued.
#[tokio::test]
async fn test_stream_waiters_do_not_starve_the_tool_category() {
    let gov = std::sync::Arc::new(ConcurrencyGovernor::new(2, 8, 4));

    // Saturate the two stream slots.
    let mut held = Vec::new();
    for _ in 0..2 {
        held.push(gov.acquire_stream().await.expect("stream slot must exist"));
    }
    assert_eq!(gov.stats().global_available, 2);

    // Two more stream requesters park on the stream semaphore.
    let mut waiters = Vec::new();
    for _ in 0..2 {
        let g = std::sync::Arc::clone(&gov);
        waiters.push(tokio::spawn(async move {
            let _permit = g
                .acquire_stream()
                .await
                .expect("waiter must eventually acquire");
        }));
    }
    // Let the spawned tasks reach their await point before inspecting.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        gov.stats().streams_available,
        0,
        "both stream slots must still be held"
    );
    assert_eq!(
        gov.stats().global_available,
        2,
        "parked stream waiters must not hold a global permit"
    );
    assert_eq!(
        gov.stats().tools_available,
        8,
        "the tool pool is untouched by the stream backlog"
    );

    // The tool category is reachable despite the stream backlog.
    let tool = tokio::time::timeout(std::time::Duration::from_millis(50), gov.acquire_tool()).await;
    assert!(
        tool.is_ok(),
        "tool acquisition must not queue behind parked stream waiters"
    );

    drop(tool);
    drop(held);
    for waiter in waiters {
        waiter.await.expect("stream waiter task must not panic");
    }
}

/// `try_acquire_stream` is the non-blocking counterpart of `acquire_stream`, and
/// like `try_acquire_tool` it must not leak the category permit when the global
/// ceiling is what refuses the request.
#[tokio::test]
async fn test_try_acquire_stream_is_non_blocking_and_leak_free() {
    let gov = ConcurrencyGovernor::new(2, 8, 2);

    let s1 = gov.try_acquire_stream().expect("first stream slot");
    let s2 = gov.try_acquire_stream().expect("second stream slot");
    assert_eq!(gov.stats().streams_available, 0);
    assert_eq!(gov.stats().global_available, 0);

    // At capacity it answers None instead of parking.
    assert!(gov.try_acquire_stream().is_none());
    drop(s1);
    drop(s2);

    // Global is the binding limit (2 globals, 8 tool slots): the refuse path
    // must give the stream permit back.
    let mut tools = Vec::new();
    for _ in 0..2 {
        tools.push(gov.acquire_tool().await.expect("tool slot"));
    }
    assert_eq!(gov.stats().global_available, 0);
    assert!(
        gov.try_acquire_stream().is_none(),
        "the global ceiling must refuse the stream even though stream slots are free"
    );
    assert_eq!(
        gov.stats().streams_available,
        2,
        "a refused try must not leak a stream permit"
    );

    drop(tools);
    assert!(gov.try_acquire_stream().is_some());
}

/// Agents with the same limits share one process-wide budget. A per-agent
/// governor let N swarm children each hold `max_streams`, so N agents put
/// N×`max_streams` streams against an endpoint with a fixed slot count.
#[test]
fn test_shared_governor_is_one_budget_per_limit_set() {
    let a = ConcurrencyGovernor::shared(16, 8, 24);
    let b = ConcurrencyGovernor::shared(16, 8, 24);
    assert!(
        std::sync::Arc::ptr_eq(&a, &b),
        "identical limits must resolve to the same shared governor"
    );

    let c = ConcurrencyGovernor::shared(4, 8, 12);
    assert!(
        !std::sync::Arc::ptr_eq(&a, &c),
        "different limits must not silently share one ceiling"
    );

    // One handle's permit is visible through the other: it is one budget.
    let held = a.try_acquire_stream().expect("stream slot");
    assert_eq!(b.stats().streams_available, 15, "the budget is shared");
    drop(held);
    assert_eq!(b.stats().streams_available, 16);
}
