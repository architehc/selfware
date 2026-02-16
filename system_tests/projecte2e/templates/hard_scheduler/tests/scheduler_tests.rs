use hard_scheduler::{next_run_at, parse_duration, should_run};

#[test]
fn parse_duration_supports_seconds_minutes_hours_days_and_spaces() {
    assert_eq!(parse_duration("45s"), Some(45));
    assert_eq!(parse_duration("15m"), Some(900));
    assert_eq!(parse_duration("2h"), Some(7200));
    assert_eq!(parse_duration("1d"), Some(86400));
    assert_eq!(parse_duration(" 5m "), Some(300));
}

#[test]
fn parse_duration_rejects_invalid_or_zero_values() {
    assert_eq!(parse_duration(""), None);
    assert_eq!(parse_duration("abc"), None);
    assert_eq!(parse_duration("9x"), None);
    assert_eq!(parse_duration("0m"), None);
}

#[test]
fn next_run_at_handles_large_values_without_panicking() {
    let now = u64::MAX - 3;
    assert_eq!(next_run_at(now, "10s"), None);
}

#[test]
fn scheduler_timing_logic_is_correct() {
    let last = 1_700_000_000;
    assert_eq!(next_run_at(last, "1d"), Some(last + 86400));

    assert!(should_run(last, last + 86400, "1d"));
    assert!(!should_run(last, last + 86399, "1d"));
}
