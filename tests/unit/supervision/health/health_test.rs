use super::*;

#[test]
fn never_pinged_is_healthy() {
    set_last_heartbeat_ms_for_test(0);
    assert!(
        heartbeat_healthy(),
        "no heartbeat wired -> healthy (bare-bones)"
    );
}

#[test]
fn fresh_heartbeat_is_healthy() {
    record_heartbeat();
    assert!(heartbeat_healthy(), "a just-recorded heartbeat is healthy");
}

#[test]
fn stale_heartbeat_is_unhealthy() {
    // A heartbeat far older than the default 900s threshold is stale.
    let old = now_ms().saturating_sub(2_000_000); // ~2000s ago
    set_last_heartbeat_ms_for_test(old.max(1));
    assert!(
        !heartbeat_healthy(),
        "a heartbeat older than the staleness threshold must be unhealthy"
    );
    // Restore a fresh heartbeat so we don't leave the global stale for other
    // tests that might observe it.
    record_heartbeat();
}
