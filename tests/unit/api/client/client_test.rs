use super::*;

#[test]
fn detect_backend_returns_string_for_unresponsive() {
    // A port that is almost certainly closed.
    let result = detect_backend("http://127.0.0.1:59999/v1");
    // Should either fail (network error) or return "unknown".
    if let Ok(label) = result {
        assert!(
            ["llama.cpp", "sglang", "vllm", "unknown"].contains(&label.as_str()),
            "unexpected backend label: {}",
            label
        );
    }
    // network error is acceptable
}

fn wall_budget_client(max_wall_secs: Option<u64>) -> ApiClient {
    let mut config = crate::config::Config {
        endpoint: "http://127.0.0.1:9/v1".to_string(), // discard port: never listens
        ..Default::default()
    };
    config.agent.max_wall_secs = max_wall_secs;
    ApiClient::new(&config).unwrap()
}

#[test]
fn wall_budget_stop_is_none_without_budget_or_within_budget() {
    // No budget configured: never stops, no anchor latched.
    let client = wall_budget_client(None);
    assert!(client.wall_budget_stop().is_none());
    assert!(client.run_wall_deadline().is_none());

    // Budget configured, run just started: within budget.
    let client = wall_budget_client(Some(600));
    assert!(client.wall_budget_stop().is_none());
}

#[test]
fn run_wall_deadline_is_latched_once_and_shared_across_clones() {
    let client = wall_budget_client(Some(600));
    let d1 = client.run_wall_deadline().expect("deadline");
    std::thread::sleep(Duration::from_millis(20));
    // A second call must NOT slide the window forward...
    let d2 = client.run_wall_deadline().expect("deadline");
    assert_eq!(d1, d2, "run deadline must be latched, not refreshed");
    // ...and clones of the client share the same anchor.
    let clone = client.clone();
    assert_eq!(Some(d1), clone.run_wall_deadline());
}

/// Force the run anchor far enough into the past that the budget is
/// already exhausted. Falls back to a real (short) sleep on platforms
/// whose monotonic clock cannot go back 120s.
async fn expire_wall_budget(client: &ApiClient, limit_secs: u64) {
    let anchor = Instant::now()
        .checked_sub(Duration::from_secs(limit_secs + 120))
        .unwrap_or_else(|| {
            // Clock cannot go back: latch "now" and the test sleeps past
            // the (1s) deadline below.
            Instant::now()
        });
    *client
        .wall_budget_start
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(anchor);
    if anchor.elapsed().as_secs() <= limit_secs {
        tokio::time::sleep(Duration::from_secs(limit_secs + 1)).await;
    }
}

#[tokio::test]
async fn wall_budget_stop_classified_as_budget_not_network() {
    let client = wall_budget_client(Some(1));
    expire_wall_budget(&client, 1).await;

    let stop = client
        .wall_budget_stop()
        .expect("budget must be reported as exhausted");
    let err = stop.to_string();
    assert!(
        err.contains("Wall-clock timeout"),
        "budget stop must carry the canonical reason, got: {}",
        err
    );
    assert!(
        stop.downcast_ref::<WallClockBudgetExceeded>().is_some(),
        "stop must be a WallClockBudgetExceeded"
    );
}

#[tokio::test]
async fn no_billable_request_is_issued_after_wall_budget_expiry() {
    let client = wall_budget_client(Some(1));
    expire_wall_budget(&client, 1).await;

    // Non-streaming path: must fail fast with the budget stop instead of
    // attempting (and retrying) a connection to the dead endpoint.
    let started = Instant::now();
    let err = client
        .chat(Vec::new(), None, ThinkingMode::Disabled)
        .await
        .expect_err("chat must fail once the wall budget is exhausted");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "budget stop must not burn retry backoff: {:?}",
        started.elapsed()
    );
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<WallClockBudgetExceeded>().is_some()),
        "expected WallClockBudgetExceeded, got: {:?}",
        err
    );

    // Streaming path: same guarantee.
    let err = client
        .chat_stream(Vec::new(), None, ThinkingMode::Disabled)
        .await
        .expect_err("chat_stream must fail once the wall budget is exhausted");
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<WallClockBudgetExceeded>().is_some()),
        "expected WallClockBudgetExceeded, got: {:?}",
        err
    );
}

#[test]
fn wall_budget_stop_message_matches_canonical_budget_reason() {
    // Same wording as Agent::enforce_hard_budgets so the failure-mode
    // classifier files the stop as a wall-budget stop, not a network error.
    let err: anyhow::Error = WallClockBudgetExceeded {
        elapsed_secs: 51,
        limit_secs: 8,
    }
    .into();
    assert_eq!(err.to_string(), "Wall-clock timeout: 51s >= 8s");
}
