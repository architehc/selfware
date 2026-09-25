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
async fn test_concurrency_governor_8_slot_server_cap_queues_9th() {
    // The llm.selfware.design SGLang deployment runs max_running_requests = 8.
    // The profile pins max_streams to 8 and max_global to 16 alongside it,
    // so 8 inflight streams still leave 8 global permits for tool execution.
    let mut config = crate::config::Config {
        model: "qwen38-flash-next".to_string(),
        ..Default::default()
    };
    let profile =
        crate::config::model_profiles::match_profile(&config.model).expect("profile must match");
    let user_explicit = crate::config::model_profiles::UserExplicitFields::default();
    crate::config::model_profiles::apply_profile(&mut config, &profile, &user_explicit);

    assert_eq!(config.concurrency.max_streams, 8);
    assert_eq!(config.concurrency.max_global, 16);

    let gov = ConcurrencyGovernor::from_config(&config.concurrency);

    let mut permits = Vec::new();
    for _ in 0..8 {
        permits.push(
            gov.acquire_stream()
                .await
                .expect("slot within 8 must be granted"),
        );
    }

    assert_eq!(gov.stats().streams_available, 0);
    assert_eq!(gov.stats().global_available, 8);

    // 9th stream request must wait / queue when all 8 slots are held
    let timeout_result =
        tokio::time::timeout(std::time::Duration::from_millis(50), gov.acquire_stream()).await;
    assert!(
        timeout_result.is_err(),
        "9th stream must queue while 8 slots are held"
    );

    // Once a permit drops, the 9th request acquires successfully
    drop(permits.pop());
    assert_eq!(gov.stats().streams_available, 1);
    let p9 = tokio::time::timeout(std::time::Duration::from_millis(50), gov.acquire_stream()).await;
    assert!(p9.is_ok(), "9th stream acquires once a permit frees up");
}

/// Parked category waiters now hold the global permit they acquired BEFORE
/// parking on the category semaphore.
///
/// This pins the acquisition order flipped by the 2026-09-21 review: the
/// order is global-then-category (total), so a waiter parked on the stream
/// semaphore keeps its global permit. That is the deliberate tradeoff that
/// makes nested acquisitions deadlock-free — see the ordering note on
/// [`ConcurrencyGovernor`]. This test documents the new invariant and the
/// inversion of the old `category-first` property (previously a parked
/// waiter held nothing; the same assertion now proves the opposite).
#[tokio::test]
async fn test_global_first_ordering_pins_parked_waiters_on_global() {
    let gov = std::sync::Arc::new(ConcurrencyGovernor::new(2, 8, 4));

    // Saturate the two stream slots (each hold: global + stream).
    let mut held = Vec::new();
    for _ in 0..2 {
        held.push(gov.acquire_stream().await.expect("stream slot must exist"));
    }
    assert_eq!(gov.stats().global_available, 2);

    // Two more stream requesters park on the stream semaphore — global first,
    // so each parked waiter holds a global permit while it waits.
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
        0,
        "parked stream waiters hold global permits (total order: global first)"
    );

    // A tool acquisition therefore parks on the global ceiling while the
    // stream backlog holds it — the accepted head-of-line cost of the total
    // order, deadlock-free: it completes as soon as a global permit frees.
    let tool_granted =
        tokio::time::timeout(std::time::Duration::from_millis(120), gov.acquire_tool()).await;
    assert!(
        tool_granted.is_err(),
        "tool acquisition must be head-of-line blocked behind parked stream waiters \
         under global-first ordering"
    );

    // Forward progress: release the stream permits; the parked waiters take
    // the stream slots (and their global permits) and finish, handing the
    // global permits back — the tool acquisition then completes. Nothing
    // wedges in any order.
    drop(held);
    for waiter in waiters {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), waiter).await;
    }
    let tool =
        tokio::time::timeout(std::time::Duration::from_millis(120), gov.acquire_tool()).await;
    assert!(
        tool.is_ok(),
        "tool acquisition completes once the stream backlog clears"
    );
    drop(tool);
}

/// Regression (2026-09-21 review): nested acquisition under a saturated
/// global pool must not wedge the governor.
///
/// Edge values that previously deadlocked: `max_global == sum of the two
/// outer holders` (2), one stream slot and one tool slot. A stream holder
/// dispatches a nested tool while a tool holder dispatches a nested stream,
/// i.e. the exact "global permits are saturated by streams that then dispatch
/// tools or nested streams" shape. With the old category-first ordering this
/// is a hold-and-wait cycle across the two categories; with the total
/// global-first order both nested calls park on the global semaphore holding
/// nothing new, and each completes as soon as its outer permit releases.
///
/// The whole scenario runs under a wall-clock guard, so a deadlock regression
/// fails the test instead of hanging it.
#[tokio::test]
async fn test_nested_acquisition_under_saturated_global_does_not_deadlock() {
    let gov = std::sync::Arc::new(ConcurrencyGovernor::new(1, 1, 2));

    // Tool holder: takes the single tool slot AND a global permit.
    let tool_outer = gov.acquire_tool().await.expect("tool slot must exist");
    // Stream holder: takes the single stream slot AND the second global permit.
    let stream_outer = gov.acquire_stream().await.expect("stream slot must exist");
    assert_eq!(
        gov.stats().global_available,
        0,
        "the global pool must be saturated by the two outer holders"
    );

    // Both holders now dispatch a nested acquisition while still holding
    // their outer permits — the deadlock shape from the review finding.
    let g1 = std::sync::Arc::clone(&gov);
    let nested_tool = tokio::spawn(async move {
        g1.acquire_tool()
            .await
            .expect("nested tool must eventually acquire")
    });
    let g2 = std::sync::Arc::clone(&gov);
    let nested_stream = tokio::spawn(async move {
        g2.acquire_stream()
            .await
            .expect("nested stream must eventually acquire")
    });

    // Give both nested calls time to reach their park point.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        gov.stats().global_available,
        0,
        "both nested acquisitions park on the saturated global pool"
    );

    // Forward progress: release the outer permits one at a time. Dropping the
    // tool permit frees a global permit AND the tool slot, so the parked
    // nested tool completes (global, then tool); the nested stream stays
    // parked (its stream slot is still held). Dropping the stream permit
    // then frees the second global and the stream slot, and the nested
    // stream completes. The pre-fix hold-and-wait cycle across the two
    // categories never resolved in any release order; the total order does.
    drop(tool_outer);
    let nested_tool = tokio::time::timeout(std::time::Duration::from_secs(2), nested_tool)
        .await
        .expect("nested tool must not deadlock once an outer permit releases")
        .expect("nested tool task must not fail");
    drop(nested_tool);

    drop(stream_outer);
    let nested_stream = tokio::time::timeout(std::time::Duration::from_secs(2), nested_stream)
        .await
        .expect("nested stream must not deadlock once an outer permit releases")
        .expect("nested stream task must not fail");
    drop(nested_stream);

    // Everything returned: all permits released, the governor is fully idle.
    let stats = gov.stats();
    assert_eq!(stats.streams_available, 1);
    assert_eq!(stats.tools_available, 1);
    assert_eq!(stats.global_available, 2);
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
