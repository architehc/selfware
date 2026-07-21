use super::*;

#[test]
fn concurrency_config_rejects_unknown_keys() {
    // Known keys parse fine.
    let ok: Result<ConcurrencyConfig, _> =
        toml::from_str("max_streams = 4\nmax_tools = 8\nmax_global = 12");
    assert!(ok.is_ok(), "valid concurrency config should parse: {ok:?}");
    // An unknown key (e.g. a typo / wrong name) must be rejected, not silently dropped.
    let bad: Result<ConcurrencyConfig, _> = toml::from_str("max_parallel_requests = 24");
    assert!(
        bad.is_err(),
        "unknown concurrency key must be rejected, got: {bad:?}"
    );
}
